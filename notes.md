# Notes

## Tracing

This _just_ needs:
 - pointer values, not any hard data on types
 - to get object headers from pointers to set mark bits
 - read only to object memory space

Safety:
 - unsafe to trace? Why would it be?
 - gray area: are we taking immutable aliases of object references?
 - gray area: this is only manipulating the object header
 - gray area: how could this be _unsafe_?
 - no: we are dereferencing pointers to get other pointers
 - yes: we are not dereferencing pointers in safe rust
 - yes: we are using cell everywhere and no threading, so safe

```rust
pub trait Trace {
    fn trace(&self);
}

// do only roots need to be scope-guarded?
pub trait Root: Trace;


pub trait AllocHeader: Trace;


impl AllocHeader for ObjectHeader {
    fn mark() {}
}

// via header to get object type
impl Trace for ObjectHeader {
    fn trace() {}
}

// through RawPtr
RawPtr::trace(&self) {}


Root::trace()
```
use std::cell::RefCell;
use std::marker::PhantomPinned;
use std::ops::Deref;
use std::pin::{pin, Pin};

// Trace //////////////////////////////
trait Trace {
    fn trace(&self);
}

// Gc<T> //////////////////////////////
#[derive(Debug, Copy, Clone)]
struct Gc<T> {
    ptr: *const T,
}

impl<T> Gc<T> {
    fn new(obj: T) -> Gc<T> {
        let p = Box::new(obj);
        Gc {
            ptr: Box::into_raw(p),
        }
    }

    fn null() -> Gc<T> {
        Gc { ptr: 0 as *const T }
    }
}

impl<T> Deref for Gc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.ptr }
    }
}

impl<T> Trace for Gc<T> {
    fn trace(&self) {
        println!("Trace for Gc<T>");
    }
}

// Rooting ////////////////////////////
#[derive(Debug)]
struct Root<T> {
    ptr: Gc<T>,
    heap: *const Heap,
    _pin: PhantomPinned,
}

impl<T> Root<T> {
    fn new(mem: &Heap, from_heap: Gc<T>) -> Root<T> {
        Root {
            ptr: from_heap,
            heap: mem,
            _pin: PhantomPinned,
        }
    }
}

impl<T> Drop for Root<T> {
    fn drop(&mut self) {
        unsafe {
            let heap = &*self.heap as &Heap;
            heap.pop_root();
        }
    }
}

impl<T> Deref for Root<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.ptr
    }
}

impl<T> Trace for Root<T> {
    fn trace(&self) {
        println!("Trace for Root<T>");
    }
}

// Heap ///////////////////////////////
#[derive(Default)]
struct Heap {
    roots: RefCell<Vec<*const dyn Trace>>,
}

impl Heap {
    fn push_root<T>(&self, root: &Root<T>) {
        self.roots
            .borrow_mut()
            .push(root as *const Root<T> as *const dyn Trace);
        println!("roots.push: {}", self.roots.borrow().len());
    }

    fn pop_root(&self) {
        self.roots.borrow_mut().pop();
        println!("roots.pop: {}", self.roots.borrow().len());
    }
}

// Test ///////////////////////////////

struct PinnedRoot<'root_lt, T> {
    root: Pin<&'root_lt Root<T>>,
}

impl<'root_lt, T> PinnedRoot<'root_lt, T> {
    fn new(root: &'root_lt Root<T>) -> PinnedRoot<'root_lt, T> {
        PinnedRoot {
            root: unsafe { Pin::new_unchecked(root) },
        }
    }
}

impl<T> Deref for PinnedRoot<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.root.deref()
    }
}

// Macro //////////////////////////////
macro_rules! root {
    ($mem:ident, $root_id:ident, $value:expr) => {
        let $root_id = Root::new(&$mem, $value);
        $mem.push_root(&$root_id);
        let $root_id = PinnedRoot::new(&$root_id);
    };
}

// Main ///////////////////////////////
fn main() {
    let heap = Heap::default();

    {
        let obj = Gc::new(String::from("foobar"));

        let root = Root::new(&heap, obj);
        heap.push_root(&root);
        let root = PinnedRoot::new(&root);

        println!("{}", *root);
    };

    {
        let obj = Gc::new(String::from("barbaz"));

        root!(heap, foo, obj);

        println!("{}", *foo);
    }
}
