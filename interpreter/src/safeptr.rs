use std::cell::Cell;
use std::fmt;
use std::ops::Deref;

use stickyimmix::{AllocObject, RawPtr};

use crate::headers::TypeList;
use crate::pointerops::AsScopedRef;
use crate::printer::Print;
use crate::taggedptr::{FatPtr, TaggedPtr, Value};

/// Type that provides a generic anchor for mutator timeslice lifetimes
// ANCHOR: DefMutatorScope
pub trait MutatorScope {}
// ANCHOR_END: DefMutatorScope

/// An untagged compile-time typed pointer with scope limited by `MutatorScope`
// ANCHOR: DefScopedPtr
pub struct ScopedPtr<'guard, T: Sized> {
    value: &'guard T,
}
// ANCHOR_END: DefScopedPtr

impl<'guard, T: Sized> ScopedPtr<'guard, T> {
    pub fn new(_guard: &'guard dyn MutatorScope, value: &'guard T) -> ScopedPtr<'guard, T> {
        ScopedPtr { value }
    }

    /// Convert the compile-time type pointer to a runtime type pointer
    pub fn as_tagged(&self, guard: &'guard dyn MutatorScope) -> TaggedScopedPtr<'guard>
    where
        FatPtr: From<RawPtr<T>>,
        T: AllocObject<TypeList>,
    {
        unsafe {
            TaggedScopedPtr::new(
                guard,
                TaggedPtr::from(FatPtr::from(RawPtr::new(self.value))),
            )
        }
    }
}

/// Anything that _has_ a scope lifetime can pass as a scope representation
impl<T: Sized> MutatorScope for ScopedPtr<'_, T> {}

impl<'guard, T: Sized> Clone for ScopedPtr<'guard, T> {
    fn clone(&self) -> ScopedPtr<'guard, T> {
       *self 
    }
}

impl<T: Sized> Copy for ScopedPtr<'_, T> {}

impl<T: Sized> Deref for ScopedPtr<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value
    }
}

impl<T: Sized + Print> fmt::Display for ScopedPtr<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.print(self, f)
    }
}

impl<T: Sized + Print> fmt::Debug for ScopedPtr<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.print(self, f)
    }
}

impl<'guard, T: Sized + PartialEq> PartialEq for ScopedPtr<'guard, T> {
    fn eq(&self, rhs: &ScopedPtr<'guard, T>) -> bool {
        self.value == rhs.value
    }
}

pub trait AsScopedPtr<T> {
    fn scoped_ptr<'scope>(&self, guard: &'scope dyn MutatorScope) -> ScopedPtr<'scope, T>;
}


/// A wrapper around untagged raw pointers for storing compile-time typed pointers in
/// data structures that are not expected to change in pointer value, i.e. once the
/// object is initialized, it remains immutably pointing at that object.
pub struct RefPtr<T: Sized> {
    inner: RawPtr<T>,
}

impl<T: Sized> RefPtr<T> {
    pub fn new_with(source: ScopedPtr<T>) -> RefPtr<T> {
        RefPtr {
            inner: RawPtr::new(source.value),
        }
    }
}

impl<T> AsScopedRef<T> for RefPtr<T> {
    fn scoped_ref<'scope>(&self, guard: &'scope dyn MutatorScope) -> &'scope T {
        self.inner.scoped_ref(guard)
    }
}

impl<T> AsScopedPtr<T> for RefPtr<T> {
    fn scoped_ptr<'scope>(&self, guard: &'scope dyn MutatorScope) -> ScopedPtr<'scope, T> {
        ScopedPtr::new(guard, self.inner.scoped_ref(guard))
    }
}

/// A wrapper around untagged raw pointers for storing compile-time typed pointers in data
/// structures with interior mutability, allowing pointers to be updated to point at different
/// target objects.
// ANCHOR: DefCellPtr
#[derive(Clone)]
pub struct CellPtr<T: Sized> {
    inner: Cell<RawPtr<T>>,
}
// ANCHOR_END: DefCellPtr

impl<T: Sized> CellPtr<T> {
    /// Construct a new CellPtr from a ScopedPtr
    pub fn new_with(source: ScopedPtr<T>) -> CellPtr<T> {
        CellPtr {
            inner: Cell::new(RawPtr::new(source.value)),
        }
    }

    // ANCHOR: DefCellPtrGet
    pub fn get<'guard>(&self, guard: &'guard dyn MutatorScope) -> ScopedPtr<'guard, T> {
        ScopedPtr::new(guard, self.inner.get().scoped_ref(guard))
    }
    // ANCHOR_END: DefCellPtrGet

    // the explicit 'guard lifetime bound to MutatorScope is omitted here since the ScopedPtr
    // carries this lifetime already so we can assume that this operation is safe
    pub fn set(&self, source: ScopedPtr<T>) {
        self.inner.set(RawPtr::new(source.value))
    }
}

impl<T: Sized> From<ScopedPtr<'_, T>> for CellPtr<T> {
    fn from(ptr: ScopedPtr<T>) -> CellPtr<T> {
        CellPtr::new_with(ptr)
    }
}

/// A _tagged_ runtime typed pointer type with scope limited by `MutatorScope` such that a `Value`
/// instance can safely be derived and accessed. This type is neccessary to derive `Value`s from.
// ANCHOR: DefTaggedScopedPtr
#[derive(Copy, Clone)]
pub struct TaggedScopedPtr<'guard> {
    ptr: TaggedPtr,
    value: Value<'guard>,
}
// ANCHOR_END: DefTaggedScopedPtr

impl<'guard> TaggedScopedPtr<'guard> {
    // This is unsafe because there is no guarantees that `ptr` is a valid pointer
    pub unsafe fn new(guard: &'guard dyn MutatorScope, ptr: TaggedPtr) -> TaggedScopedPtr<'guard> {
        TaggedScopedPtr {
            ptr,
            value: FatPtr::from(ptr).as_value(guard),
        }
    }

    pub fn number(guard: &'guard dyn MutatorScope, num: isize) -> TaggedScopedPtr<'guard> {
        let ptr_val = TaggedPtr::number(num);
        TaggedScopedPtr {
            ptr: ptr_val,
            value: FatPtr::from(ptr_val).as_value(guard),
        }
    }

    pub fn nil(guard: &'guard dyn MutatorScope) -> TaggedScopedPtr<'guard> {
        TaggedScopedPtr {
            ptr: TaggedPtr::nil(),
            value: FatPtr::Nil.as_value(guard),
        }
    }

    pub fn value(&self) -> Value<'guard> {
        self.value
    }
}

/// Anything that _has_ a scope lifetime can pass as a scope representation. `Value` also implements
/// `MutatorScope` so this is largely for consistency.
impl MutatorScope for TaggedScopedPtr<'_> {}

impl<'guard> Deref for TaggedScopedPtr<'guard> {
    type Target = Value<'guard>;

    fn deref(&self) -> &Value<'guard> {
        &self.value
    }
}

impl fmt::Display for TaggedScopedPtr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl fmt::Debug for TaggedScopedPtr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<'guard> PartialEq for TaggedScopedPtr<'guard> {
    fn eq(&self, rhs: &TaggedScopedPtr<'guard>) -> bool {
        self.ptr == rhs.ptr
    }
}

/// A wrapper around the runtime typed `TaggedPtr` for storing pointers in data structures with
/// interior mutability, allowing pointers to be updated to point at different target objects.
// ANCHOR: DefTaggedCellPtr
#[derive(Clone)]
pub struct TaggedCellPtr {
    inner: Cell<TaggedPtr>,
}
// ANCHOR_END: DefTaggedCellPtr

impl TaggedCellPtr {
    /// Construct a new Nil TaggedCellPtr instance
    pub fn new_nil() -> TaggedCellPtr {
        TaggedCellPtr {
            inner: Cell::new(TaggedPtr::nil()),
        }
    }

    /// Construct a new TaggedCellPtr from a TaggedScopedPtr
    pub fn new_with(source: TaggedScopedPtr) -> TaggedCellPtr {
        TaggedCellPtr {
            inner: Cell::new(source.ptr),
        }
    }

    /// Construct a new TaggedCellPtr from another
    pub fn new_copy(source: &TaggedCellPtr) -> TaggedCellPtr {
        TaggedCellPtr {
            inner: Cell::new(source.inner.get()),
        }
    }

    /// Return the pointer as a `TaggedScopedPtr` type that carries a copy of the `TaggedPtr` and
    /// a `Value` type for both copying and access convenience
    // ANCHOR: DefTaggedCellPtrGet
    pub fn get<'guard>(&self, guard: &'guard dyn MutatorScope) -> TaggedScopedPtr<'guard> {
        unsafe { TaggedScopedPtr::new(guard, self.inner.get()) }
    }
    // ANCHOR_END: DefTaggedCellPtrGet

    /// Set this pointer to point at the same object as a given `TaggedScopedPtr` instance
    /// The explicit 'guard lifetime bound to MutatorScope is omitted here since the TaggedScopedPtr
    /// carries this lifetime already so we can assume that this operation is safe
    pub fn set(&self, source: TaggedScopedPtr) {
        self.inner.set(source.ptr)
    }

    /// Take the pointer of another `TaggedCellPtr` and set this instance to point at that object too
    pub fn copy_from(&self, _guard: &'_ dyn MutatorScope, src: &TaggedCellPtr) {
        self.inner.set(src.inner.get());
    }

    /// Set another instance to hold the same pointer as this instance
    pub fn copy_into(&self, _guard: &'_ dyn MutatorScope, dest: &TaggedCellPtr) {
        dest.inner.set(self.inner.get());
    }

    /// Return true if the pointer is nil
    pub fn is_nil(&self) -> bool {
        self.inner.get().is_nil()
    }

    /// Set this pointer to nil
    pub fn set_to_nil(&self) {
        self.inner.set(TaggedPtr::nil())
    }

    /// Set this pointer to another TaggedPtr
    // TODO DEPRECATE IF POSSIBLE
    //  - this is only used to set non-object tagged values and should be replaced/renamed
    // XXX: this should be unsafe
    pub fn set_to_ptr(&self, _guard: &'_ dyn MutatorScope, ptr: TaggedPtr) {
        self.inner.set(ptr)
    }

    /// Return the raw TaggedPtr from within
    // TODO DEPRECATE IF POSSIBLE
    pub fn get_ptr(&self, _guard: &'_ dyn MutatorScope) -> TaggedPtr {
        self.inner.get()
    }
}

impl From<TaggedScopedPtr<'_>> for TaggedCellPtr {
    fn from(ptr: TaggedScopedPtr) -> TaggedCellPtr {
        TaggedCellPtr::new_with(ptr)
    }
}
