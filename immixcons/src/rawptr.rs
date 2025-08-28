use std::ptr::NonNull;

/// A container for a bare pointer to an object of type `T`.
/// At this level, compile-time type information is still
/// part of the type.
// ANCHOR: DefRawPtr
pub struct RawPtr<T: Sized> {
    ptr: NonNull<T>,
}
// ANCHOR_END: DefRawPtr

impl<T: Sized> RawPtr<T> {
    /// Create a new RawPtr from a bare pointer
    pub fn new(ptr: *const T) -> RawPtr<T> {
        RawPtr {
            ptr: unsafe { NonNull::new_unchecked(ptr as *mut T) },
        }
    }

    /// Get the raw `*const` pointer to the object.
    pub fn as_ptr(self) -> *const T {
        self.ptr.as_ptr()
    }

    /// Get the pointer value as a word-sized integer
    pub fn addr(self) -> usize {
        self.ptr.as_ptr() as usize
    }

    /// Get the pointer as a null-type value
    // XXX: is this really needed? Any added benefit?
    pub fn as_untyped(self) -> NonNull<()> {
        self.ptr.cast()
    }

    /// Get a `&` reference to the object. Unsafe because there are no guarantees at this level
    /// about the internal pointer's validity.
    pub unsafe fn as_ref(&self) -> &T {
        self.ptr.as_ref()
    }
}

impl<T: Sized> Clone for RawPtr<T> {
    fn clone(&self) -> RawPtr<T> {
        RawPtr { ptr: self.ptr }
    }
}

impl<T: Sized> Copy for RawPtr<T> {}

impl<T: Sized> PartialEq for RawPtr<T> {
    fn eq(&self, other: &RawPtr<T>) -> bool {
        self.ptr == other.ptr
    }
}
