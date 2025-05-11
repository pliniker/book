use std::cell::RefCell;
use std::marker::PhantomPinned;
use std::ops::{Deref, DerefMut};
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

    fn as_usize(&self) -> usize {
        self.ptr as usize
    }
}

impl<T> Deref for Gc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.ptr }
    }
}

impl<T> DerefMut for Gc<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *(self.ptr as *mut T) }
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
    _pin: PhantomPinned,
}

impl<T> Root<T> {
    fn new(from_heap: Gc<T>) -> Root<T> {
        Root {
            ptr: from_heap,
            _pin: PhantomPinned,
        }
    }
}

impl<T> Drop for Root<T> {
    fn drop(&mut self) {}
}

impl<T> Deref for Root<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.ptr
    }
}

impl<T> DerefMut for Root<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.ptr
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
    allocations: Vec<usize>,
}

impl Heap {
    fn alloc<T>(&mut self, obj: T) -> Gc<T> {
        let a = Gc::new(obj);
        self.allocations.push(a.as_usize());
        a
    }
}

// Test ///////////////////////////////

struct PinnedRoot<'root_lt, T> {
    root: Pin<&'root_lt mut Root<T>>,
}

impl<'root_lt, T> PinnedRoot<'root_lt, T> {
    fn new(root: &'root_lt mut Root<T>) -> PinnedRoot<'root_lt, T> {
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
    ($root_id:ident, $value:expr) => {
        let mut $root_id = Root::new($value);
        #[allow(unused_mut)]
        let mut $root_id = PinnedRoot::new(&mut $root_id);
    };
}

// Main ///////////////////////////////
fn main() {
    let mut heap = Heap::default();

    {
        let obj = heap.alloc(String::from("foobar"));

        let mut root = Root::new(obj);
        let mut root = PinnedRoot::new(&mut root);

        println!("{}", *root);
    };

    {
        root!(foo, heap.alloc(String::from("barbaz")));

        println!("{}", *foo);
    }
}
