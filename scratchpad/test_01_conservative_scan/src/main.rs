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
use std::ops::Deref;
use std::slice::from_raw_parts;

trait Trace {
    fn trace(&self, objects: &mut Vec<usize>) {}
}

struct Gc<T: Trace + Sized> {
    inner: *const T,
}

impl<T: Trace + Sized> Gc<T> {
    fn new(object: *const T) -> Gc<T> {
        Gc { inner: object }
    }
}

impl<T: Trace + Sized> Deref for Gc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.inner as &T }
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
    fn new<T: Trace>(raw: *const T, tobj: Box<dyn Trace>) -> HeapItem {
        HeapItem {
            mark: false,
            address: raw as usize,
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

        let tobj: Box<dyn Trace> = obj;

        // put Trace trait object into heap object list
        let gc_ref = HeapItem::new(raw_ptr, tobj);
        self.objects.insert(raw_ptr as usize, gc_ref);

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

        if stack_top < stack_base {
            (stack_top, stack_base) = (stack_base, stack_top);
        }

        let word_size = size_of::<usize>();
        let stack_len = (stack_top - stack_base) / word_size;
        let slice = unsafe { from_raw_parts(stack_base as *const usize, stack_len) };

        for stack_item in slice {
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
                println!("DROP {:x}", heap_item.address);
                drop(heap_item.object);
            }
        });
    }

    fn gc(&mut self) {
        self.scan();
        self.mark();
        self.collect();
        self.scan.clear();
    }
}

impl Drop for Memory {
    fn drop(&mut self) {
        let temp = std::mem::take(&mut self.objects);

        temp.into_values().for_each(|heap_item| {
            println!("DROP {:x}", heap_item.address);
            drop(heap_item.object);
        });
    }
}

fn main() {
    let mut mem = Memory::new();

    let foo = mem.alloc(HeapString::from("foosball"));

    for i in 0x0..0xF {
        let bar = mem.alloc(HeapString::from("foobar"));
    }

    mem.gc();

    foo.print();

    mem.gc();
}
