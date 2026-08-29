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
            ptr: NonNull::new(ptr as *mut T).expect("Reconstructed RawPtr<T> from a null value!"),
        }
    }

    /// Get the raw `*const` pointer to the object.
    pub fn as_ptr(self) -> *const T {
        self.ptr.as_ptr()
    }

    /// Get the pointer value as a word-sized integer
    pub fn addr(self) -> usize {
        self.ptr.as_ptr().addr()
    }

    /// Get the pointer as a null-type value
    pub fn as_untyped(self) -> RawPtr<()> {
        RawPtr {
            ptr: self.ptr.cast(),
        }
    }

    /// Cast the pointer to a different type
    /// Safety: none. Attempting to dereference the pointer will always be
    /// unsafe.
    pub fn cast<U: Sized>(self) -> RawPtr<U> {
        RawPtr {
            ptr: self.ptr.cast(),
        }
    }

    /// Get a `&` reference to the object. Unsafe because there are no guarantees at this level
    /// about the internal pointer's validity.
    pub unsafe fn as_ref(&self) -> &T {
        unsafe { self.ptr.as_ref() }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_ptr_addr_clone_eq_untyped_as_ref_on_copyable() {
        let x: usize = 0x1234_5678;
        let rp = RawPtr::new(&x as *const usize);

        // as_ptr & addr
        assert_eq!(rp.as_ptr(), &x as *const usize);
        assert_eq!(rp.addr(), (&x as *const usize) as usize);

        // Clone and Copy behavior
        let rp2 = rp.clone();
        assert!(rp2 == rp);
        let rp3 = rp;
        assert!(rp3 == rp2);

        // as_untyped
        let untyped = rp.as_untyped();
        assert_eq!(untyped.as_ptr() as usize, rp.as_ptr() as usize);

        // as_ref is unsafe
        unsafe {
            let r = rp.as_ref();
            assert_eq!(*r, x);
        }
    }

    #[test]
    fn test_as_ref_for_noncopy_types() {
        let s = String::from("hello world");
        let rp = RawPtr::new(&s as *const String);

        unsafe {
            let r = rp.as_ref();
            assert_eq!(r, &s);
        }

        // Also ensure addr points correctly
        assert_eq!(rp.addr(), (&s as *const String) as usize);
    }
}
