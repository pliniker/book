/*
 Here we will:
 - a simple allocator that keeps a copy of the object in a Vec
 - a stack scanner, that will
   - mark objects as live
   - drop dead objects
*/
use libc::getcontext;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::hint::black_box;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};
use std::slice::from_raw_parts;

trait Trace {
    /// Give me all your pointers
    fn trace(&self, _objects: &mut Vec<usize>) {}
}

struct Gc<T: Trace + Sized> {
    inner: *const T,
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
}

impl<T: Trace + Sized> Deref for Gc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.inner as &T }
    }
}

impl<T: Trace + Sized> DerefMut for Gc<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *(self.inner as *mut T) }
    }
}

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

struct HeapArray<T: Trace> {
    value: Vec<Gc<T>>,
}

impl<T: Trace> HeapArray<T> {
    fn new() -> HeapArray<T> {
        HeapArray { value: Vec::new() }
    }

    fn push(&mut self, object: Gc<T>) {
        self.value.push(object);
    }
}

impl<T: Trace> Trace for HeapArray<T> {
    fn trace(&self, objects: &mut Vec<usize>) {
        for item in self.value.iter() {
            objects.push(item.addr())
        }
    }
}

struct StackItem {
    value: usize,
}

impl StackItem {
    fn new(value: usize) -> StackItem {
        StackItem { value }
    }
}

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

struct MemoryInner<'memory> {
    objects: BTreeMap<usize, HeapItem<'memory>>,
    scan: Vec<StackItem>,
}

struct Memory<'memory> {
    inner: RefCell<MemoryInner<'memory>>,
    base: usize,
}

impl<'memory> Memory<'memory> {
    fn new() -> Memory<'memory> {
        let inner = MemoryInner::<'memory> {
            objects: BTreeMap::new(),
            scan: Vec::new(),
        };
        Memory {
            inner: RefCell::new(inner),
            base: 0xbeefbabe,
        }
    }

    fn alloc<T: Trace + 'memory>(&self, object: T) -> Gc<T> {
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

    #[no_mangle]
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

    fn enter<'guard, F>(&'guard self, run: F)
    where
        F: FnOnce(&MutatorView<'memory, 'guard>),
        'memory: 'guard,
    {
        let delegate = MutatorView::new(self);
        run(&delegate);
    }
}

impl<'memory> Drop for Memory<'memory> {
    fn drop(&mut self) {
        let mut inner = self.inner.borrow_mut();

        let temp = std::mem::take(&mut inner.objects);

        temp.into_values().for_each(|heap_item| {
            println!("<gc> EXIT {:x}", heap_item.address);
            drop(heap_item.object);
        });
    }
}

trait MutatorScope {}

struct MutatorView<'memory, 'guard> {
    mem: &'guard Memory<'memory>,
}

impl<'memory, 'guard> MutatorScope for MutatorView<'memory, 'guard> {}

impl<'memory, 'guard> MutatorView<'memory, 'guard> {
    fn new(mem: &'guard Memory<'memory>) -> Self {
        MutatorView::<'memory, 'guard> { mem }
    }

    fn alloc<T: 'memory>(&self, value: T) -> Gc<T>
    where
        T: Trace,
    {
        self.mem.alloc(value)
    }

    fn gc(&self) {
        self.mem.gc();
    }
}

fn test_do_some_stuff(mem: &MutatorView) {
    let mut array = mem.alloc(HeapArray::<HeapString>::new());
    array.debug();

    for _ in 0x0..0xF {
        let bar = mem.alloc(HeapString::from("foobar"));
        array.push(bar);
    }
    println!("test_do_some_stuff");
}

fn main() {
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
    });
}
