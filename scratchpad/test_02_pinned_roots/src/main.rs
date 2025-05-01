/*
 Here we will:
 - a simple allocator that keeps a copy of the object in a Vec
 - a stack scanner, that will
   - mark objects as live
   - drop dead objects
*/
use libc::getcontext;
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

struct HeapItem {
    mark: bool,
    address: usize,
    object: Box<dyn Trace>,
}

impl HeapItem {
    fn new(addr: usize, tobj: Box<dyn Trace>) -> HeapItem {
        HeapItem {
            mark: false,
            address: addr,
            object: tobj,
        }
    }
}

struct Memory {
    objects: BTreeMap<usize, HeapItem>,
    scan: Vec<StackItem>,
    base: usize,
}

impl Memory {
    fn new() -> Memory {
        Memory {
            objects: BTreeMap::new(),
            scan: Vec::new(),
            base: 0xbeefbabe,
        }
    }

    fn alloc<T: Trace + 'static>(&mut self, object: T) -> Gc<T> {
        let obj: Box<T> = Box::new(object);
        let raw_ptr = &*obj as *const T;
        let addr = raw_ptr.addr();

        let tobj: Box<dyn Trace> = obj;

        // put Trace trait object into heap object list
        let gc_ref = HeapItem::new(addr, tobj);
        self.objects.insert(addr, gc_ref);

        println!("(alloc) {:x}", addr);
        Gc::new(raw_ptr)
    }

    #[no_mangle]
    fn scan(&mut self) {
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

        for stack_item in slice {
            // if *stack_item != 0 {
            //     println!("[stack] {:x}", *stack_item);
            // }
            self.scan.push(StackItem::new(*stack_item));
        }

        black_box(&context);
    }

    fn mark(&mut self) {
        let mut heap_scan: Vec<usize> = Vec::new();

        // #1 scan the stack for heap objects
        for item in self.scan.drain(..) {
            let possible_address = item.value;

            if let Some(_) = self.objects.get(&possible_address) {
                heap_scan.push(possible_address);
                println!("[root] {:x}", possible_address);
            }
        }

        // #2 trace the heap object graph
        while heap_scan.len() > 0 {
            if let Some(heap_address) = heap_scan.pop() {
                if let Some(heap_item) = self.objects.get_mut(&heap_address) {
                    heap_item.mark = true;
                    heap_item.object.trace(&mut heap_scan);
                }
            }
        }
    }

    fn collect(&mut self) {
        let temp = std::mem::take(&mut self.objects);

        temp.into_values().for_each(|mut heap_item| {
            if heap_item.mark {
                heap_item.mark = false;
                self.objects.insert(heap_item.address, heap_item);
            } else {
                println!("<gc> DROP {:x}", heap_item.address);
                drop(heap_item.object);
            }
        });
    }

    fn gc(&mut self) {
        println!("<gc>");
        self.scan();
        self.mark();
        self.collect();
        self.scan.clear();
    }

    fn enter<F>(&mut self, run: F)
    where
        F: FnOnce(&mut MutatorView),
    {
        let mut delegate = MutatorView::new(self);
        run(&mut delegate);
    }
}

impl Drop for Memory {
    fn drop(&mut self) {
        let temp = std::mem::take(&mut self.objects);

        temp.into_values().for_each(|heap_item| {
            println!("<gc> EXIT {:x}", heap_item.address);
            drop(heap_item.object);
        });
    }
}

trait MutatorScope {}

struct MutatorView<'memory> {
    mem: &'memory mut Memory,
}

impl<'memory> MutatorScope for MutatorView<'memory> {}

impl<'memory> MutatorView<'memory> {
    fn new(mem: &'memory mut Memory) -> Self {
        MutatorView { mem }
    }

    fn alloc<T: 'static>(&mut self, value: T) -> Gc<T>
    where
        T: Trace,
    {
        self.mem.alloc(value)
    }

    fn gc(&mut self) {
        self.mem.gc();
    }
}

fn test_do_some_stuff(mem: &mut MutatorView) {
    let mut array = mem.alloc(HeapArray::<HeapString>::new());
    array.debug();

    for _ in 0x0..0xF {
        let bar = mem.alloc(HeapString::from("foobar"));
        array.push(bar);
    }
    println!("test_do_some_stuff");
}

fn main() {
    let mut arena = Memory::new();

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
