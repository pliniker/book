/*
A heap needs to be blocked from _general_ access.

Something needs to mediate between native-Rust and interpreter heap.

Impossible to prevent CellPtr and TaggedCellPtr leaks.

Define Root types
- roots are technically taken in vm opcode handling
- escapable roots, that can be taken beyond the vm - need a name
  - pinnable?

- Pin<T>

*/

use std::cell::RefCell;
use std::ops::Deref;
use std::pin::Pin;

//////////////////////// Rootability //
mod permaseal {
    pub trait _PermaSeal {}
    impl _PermaSeal for super::Allow {}
    impl _PermaSeal for super::Deny {}
}

trait _AccessControl: permaseal::_PermaSeal {}
struct Allow {}
struct Deny {}
impl _AccessControl for Allow {}
impl _AccessControl for Deny {}

trait RootPermission {
    type Rootable: _AccessControl;
}

//////////////////////// Trace //
trait Trace: RootPermission {
    fn trace(&self);
}

//////////////////////// Heap Allocatable Object //
#[derive(Copy, Clone, Debug)]
struct HeapInt {
    value: i64,
}

impl HeapInt {
    fn new(val: i64) -> HeapInt {
        HeapInt { value: val }
    }
}

impl RootPermission for HeapInt {
    type Rootable = Deny;
}

impl Trace for HeapInt {
    fn trace(&self) {}
}

//////////////////////// Heap Pointer //
#[derive(Copy, Clone, Debug)]
struct Gc<T: RootPermission<Rootable = Deny>> {
    ptr: *const T,
}

impl<'guard, T: RootPermission<Rootable = Deny>> Gc<T> {
    fn new(object: *const T) -> Gc<T> {
        Gc { ptr: object }
    }

    fn deref(&self, _guard: &'guard MemoryRef) -> &'guard T {
        unsafe { &*self.ptr as &'guard T }
    }
}

impl<T: RootPermission<Rootable = Deny>> RootPermission for Gc<T> {
    type Rootable = Deny;
}

impl<T: RootPermission<Rootable = Deny>> Trace for Gc<T> {
    fn trace(&self) {}
}

//////////////////////// Rooted Heap Pointer //
#[derive(Copy, Clone, Debug)]
struct Root<T: RootPermission<Rootable = Deny>> {
    value: Gc<T>,
}

impl<T> Root<T>
where
    T: RootPermission<Rootable = Deny>,
{
    fn new(object: Gc<T>) -> Root<T> {
        Root { value: object }
    }
}

impl<T> RootPermission for Root<T>
where
    T: RootPermission<Rootable = Deny>,
{
    type Rootable = Allow;
}

impl<T: RootPermission<Rootable = Deny>> Trace for Root<T> {
    fn trace(&self) {}
}

//////////////////////// Heap Mutation Environment //
trait Mutator: Sized {
    type Input: RootPermission<Rootable = Allow>;
    type Output: RootPermission<Rootable = Allow>;

    fn run(&self, mem: &MemoryRef, input: Self::Input) -> Self::Output;
}

struct Memory {}

impl Memory {
    fn alloc<T: RootPermission<Rootable = Deny>>(&self, value: T) -> Gc<T> {
        let boxed = Box::new(value);
        Gc::new(Box::into_raw(boxed))
    }

    fn mutate<F>(&self)
    where
        F: FnOnce(),
    {
    }
}

struct MemoryRef<'scope> {
    heap: &'scope Memory,
}

impl Deref for MemoryRef<'_> {
    type Target = Memory;
    fn deref(&self) -> &Memory {
        self.heap
    }
}

struct Mutant {}

impl Mutator for Mutant {
    type Input = IO;
    type Output = IO;

    fn run(&self, mem: &MemoryRef, input: Self::Input) -> Self::Output {
        let heapint = mem.alloc(HeapInt::new(3));
        heapint.trace();
        let root = Root::new(heapint);
        println!("{:?}", heapint.deref(mem));
        input.escaped.borrow_mut().push(root);
        input
    }
}

struct IO {
    escaped: RefCell<Vec<Root<HeapInt>>>,
}

impl RootPermission for IO {
    type Rootable = Allow;
}

//////////////////////// Main //
fn main() {
    let m = Memory {};

    let scope = MemoryRef { heap: &m };

    let mutant = Mutant {};

    let io = IO {
        escaped: RefCell::new(Vec::new()),
    };

    let result = mutant.run(&scope, io);

    for value in result.escaped.borrow().iter() {
        println!("result: {:?}", value);
    }
}
