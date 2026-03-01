/// Miscelaneous pointer operations
use immixcons::RawPtr;

use crate::safeptr::MutatorScope;

// Pointer tag values and masks using the lowest 2 bits
// ANCHOR: TaggedPtrTags
pub const TAG_MASK: usize = 0x3;
pub const TAG_SYMBOL: usize = 0x0;
pub const TAG_PAIR: usize = 0x1;
pub const TAG_OBJECT: usize = 0x2;
pub const TAG_NUMBER: usize = 0x3;
const PTR_MASK: usize = !0x3;
// ANCHOR_END: TaggedPtrTags

/// Return the tag from the given word
pub fn get_tag(tagged_word: usize) -> usize {
    tagged_word & TAG_MASK
}

/// Pointer tagging operations on RawPtr<T>
// ANCHOR: DefTagged
pub trait Tagged<T> {
    fn tag(self, tag: usize) -> RawPtr<T>;
    fn untag(from: RawPtr<T>) -> RawPtr<T>;
}

impl<T> Tagged<T> for RawPtr<T> {
    fn tag(self, tag: usize) -> RawPtr<T> {
        RawPtr::new((self.addr() | tag) as *mut T)
    }

    fn untag(from: RawPtr<T>) -> RawPtr<T> {
        RawPtr::new((from.as_ptr() as usize & PTR_MASK) as *const T)
    }
}
// ANCHOR_END: DefTagged

/// For accessing a pointer target, given a lifetime
// ANCHOR: DefScopedRef
pub trait AsScopedRef<T> {
    fn scoped_ref<'scope>(&self, guard: &'scope dyn MutatorScope) -> &'scope T;
}

impl<T> AsScopedRef<T> for RawPtr<T> {
    fn scoped_ref<'scope>(&self, _guard: &'scope dyn MutatorScope) -> &'scope T {
        unsafe { &*self.as_ptr() }
    }
}
// ANCHOR_END: DefScopedRef

/// For accessing raw pointer values
pub trait DebugPtr {
    fn addr(&self) -> usize;
}
