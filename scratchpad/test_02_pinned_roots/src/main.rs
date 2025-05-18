// Regarding pinning...
//
// - mutable variables can be std::mem::replace()'d etc
// - immutable variables can not
//
// Pinning requires shadowing every root to hold it in place
//
// - the assumption is that a root might escape from its scope
// - The Gc<T> type is most at risk of being escaped by being stored somewhere that can escape
// - An immutable Root<'lifetime, T> is not at risk
//
// Thus the fix to preventing roots escaping is to make all data structures provide a root-based API
//
// The interior mutability pattern must be strictly adhered to. Is there a way to enforce???
//
// TODO: arena for reference counted input/output pointers

use libc::getcontext;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::hint::black_box;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::slice::from_raw_parts;

///////////////////////
trait Trace {
    /// Give me all your pointers
    fn trace(&self, _objects: &mut Vec<usize>) {}
}

///////////////////////
struct Gc<T: Trace + Sized> {
    inner: *const T,
}

impl<T: Trace + Sized> Copy for Gc<T> {}

impl<T: Trace + Sized> Clone for Gc<T> {
    fn clone(&self) -> Self {
        Gc::new(self.inner)
    }
}

impl<T: Trace + Sized> Gc<T> {
    fn new(object: *const T) -> Gc<T> {
        Gc { inner: object }
    }

    fn addr(&self) -> usize {
        self.inner.addr()
    }

    fn debug(&self) {
        println!("object {:x}", self.inner.addr());
    }

    unsafe fn as_ref(&self) -> &T {
        &*self.inner as &T
    }
}

///////////////////////
struct HeapString {
    value: String,
}

impl HeapString {
    fn from(from: &str) -> HeapString {
        HeapString {
            value: String::from(from),
        }
    }

    fn print(&self) {
        println!("{}", &self.value);
    }
}

impl Trace for HeapString {
    fn trace(&self, _objects: &mut Vec<usize>) {}
}

///////////////////////
struct HeapArray<T: Trace + Sized> {
    value: RefCell<Vec<Gc<T>>>,
}

impl<T: Trace + Sized> HeapArray<T> {
    fn new() -> HeapArray<T> {
        HeapArray {
            value: RefCell::new(Vec::new()),
        }
    }

    fn push(&self, object: &Root<T>) {
        self.value.borrow_mut().push(object.var);
    }
}

impl<T: Trace + Sized> Trace for HeapArray<T> {
    fn trace(&self, objects: &mut Vec<usize>) {
        for item in self.value.borrow().iter() {
            objects.push(item.addr())
        }
    }
}

///////////////////////
struct StackItem {
    value: usize,
}

impl StackItem {
    fn new(value: usize) -> StackItem {
        StackItem { value }
    }
}

///////////////////////
struct HeapItem<'memory> {
    mark: bool,
    address: usize,
    object: Box<dyn Trace + 'memory>,
}

impl<'memory> HeapItem<'memory> {
    fn new(addr: usize, tobj: Box<dyn Trace + 'memory>) -> HeapItem<'memory> {
        HeapItem {
            mark: false,
            address: addr,
            object: tobj,
        }
    }
}

///////////////////////
struct MemoryInner<'heap> {
    objects: BTreeMap<usize, HeapItem<'heap>>,
    scan: Vec<StackItem>,
}

///////////////////////
struct Memory<'heap> {
    inner: RefCell<MemoryInner<'heap>>,
    base: usize,
}

impl<'heap> Memory<'heap> {
    fn new() -> Memory<'heap> {
        let inner = MemoryInner::<'heap> {
            objects: BTreeMap::new(),
            scan: Vec::new(),
        };
        Memory {
            inner: RefCell::new(inner),
            base: 0xbeefbabe,
        }
    }

    fn alloc_raw<T: Trace + 'heap>(&self, object: T) -> Gc<T> {
        let obj: Box<T> = Box::new(object);
        let raw_ptr = &*obj as *const T;
        let addr = raw_ptr.addr();

        let tobj: Box<dyn Trace> = obj;

        // put Trace trait object into heap object list
        let gc_ref = HeapItem::new(addr, tobj);
        self.inner.borrow_mut().objects.insert(addr, gc_ref);

        println!("(alloc) {:x}", addr);
        Gc::new(raw_ptr)
    }

    fn alloc<'mem, T: Trace + 'heap>(&'mem self, value: T) -> Root<'mem, T> {
        Root::new(self.alloc_raw(value))
    }

    fn scan(&self) {
        let mut context = MaybeUninit::zeroed();
        let result = unsafe { getcontext(context.as_mut_ptr()) };
        if result != 0 {
            panic!("could not get thread context!");
        }

        let stack_top_marker: usize = 0xbeefd00d;

        let mut stack_top = (&stack_top_marker as *const usize).addr();
        let mut stack_base = (&self.base as *const usize).addr();

        let word_size = size_of::<usize>();

        if stack_top < stack_base {
            (stack_top, stack_base) = (stack_base + word_size, stack_top);
        }

        let stack_len = (stack_top - stack_base) / word_size;
        let slice = unsafe { from_raw_parts(stack_base as *const usize, stack_len) };

        let stack_scan = &mut self.inner.borrow_mut().scan;

        for stack_item in slice {
            // if *stack_item != 0 {
            //     println!("[stack] {:x}", *stack_item);
            // }
            stack_scan.push(StackItem::new(*stack_item));
        }

        black_box(&context);
    }

    fn mark(&self) {
        let mut heap_scan: Vec<usize> = Vec::new();

        let mut inner = self.inner.borrow_mut();

        let mut scan = std::mem::take(&mut inner.scan);

        // #1 scan the stack for heap objects
        for item in scan.drain(..) {
            let possible_address = item.value;

            if let Some(_) = inner.objects.get(&possible_address) {
                heap_scan.push(possible_address);
                println!("[root] {:x}", possible_address);
            }
        }

        // #2 trace the heap object graph
        while heap_scan.len() > 0 {
            if let Some(heap_address) = heap_scan.pop() {
                if let Some(heap_item) = inner.objects.get_mut(&heap_address) {
                    heap_item.mark = true;
                    heap_item.object.trace(&mut heap_scan);
                }
            }
        }
    }

    fn collect(&self) {
        let mut inner = self.inner.borrow_mut();

        let temp = std::mem::take(&mut inner.objects);

        temp.into_values().for_each(|mut heap_item| {
            if heap_item.mark {
                heap_item.mark = false;
                inner.objects.insert(heap_item.address, heap_item);
            } else {
                println!("<gc> DROP {:x}", heap_item.address);
                drop(heap_item.object);
            }
        });
    }

    fn gc(&self) {
        println!("<gc>");
        self.scan();
        self.mark();
        self.collect();
    }

    fn enter<'mutator, F>(&'mutator self, mutant: F)
    where
        F: FnOnce(&MutatorView<'heap, 'mutator>),
        //'heap: 'mutator,
    {
        let delegate = MutatorView::new(self);
        mutant(&delegate);
    }
}

impl<'heap> Drop for Memory<'heap> {
    fn drop(&mut self) {
        let mut inner = self.inner.borrow_mut();

        let temp = std::mem::take(&mut inner.objects);

        temp.into_values().for_each(|heap_item| {
            println!("<gc> EXIT {:x}", heap_item.address);
            drop(heap_item.object);
        });
    }
}

///////////////////////
struct MutatorView<'heap, 'mutator> {
    mem: &'mutator Memory<'heap>,
}

impl<'heap, 'mutator> MutatorView<'heap, 'mutator> {
    fn new(mem: &'mutator Memory<'heap>) -> Self {
        MutatorView { mem }
    }

    fn alloc<'mem, T: Trace + 'heap>(&'mem self, value: T) -> Root<'mem, T>
    where
        T: Trace,
    {
        self.mem.alloc(value)
    }

    fn gc(&self) {
        self.mem.gc();
    }
}

///////////////////////
struct Root<'root, T: Trace> {
    var: Gc<T>,
    p: PhantomData<&'root T>,
}

impl<'root, T: Trace> Root<'root, T> {
    fn new(var: Gc<T>) -> Root<'root, T> {
        Root {
            var,
            p: PhantomData,
        }
    }

    fn debug(&self) {
        self.var.debug();
    }
}

impl<'root, T: Trace> Deref for Root<'root, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { self.var.as_ref() }
    }
}

///////////////////////
fn test_do_some_stuff(mem: &MutatorView) {
    let array = mem.alloc(HeapArray::<HeapString>::new());
    array.debug();

    for _ in 0x0..0xF {
        let bar = mem.alloc(HeapString::from("foobar"));
        array.push(&bar);
    }
    println!("test_do_some_stuff");
}

fn main() {
    // let mut escapees = Vec::new();
    {
        let arena = Memory::new();

        arena.enter(|mem| {
            let foo = mem.alloc(HeapString::from("foosball"));

            for _ in 0x0..0xF {
                let _bar = mem.alloc(HeapString::from("foobar"));
            }
            mem.gc();

            test_do_some_stuff(mem);

            foo.debug();
            foo.print();

            let bar = mem.alloc(HeapString::from("barbell"));
            bar.debug();

            mem.gc();

            //escapees.push(foo);
        });
    }

    // for item in escapees.iter() {
    //     item.print();
    // }
}
