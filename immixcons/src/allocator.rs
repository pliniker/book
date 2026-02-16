use crate::constants;
use crate::rawptr::RawPtr;
use std::mem::size_of;
use std::ptr::NonNull;

/// An allocation error type
// ANCHOR: DefAllocError
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum AllocError {
    /// Some attribute of the allocation, most likely the size requested,
    /// could not be fulfilled
    BadRequest,
    /// Out of memory - allocating the space failed
    OOM,
}
// ANCHOR_END: DefAllocError

/// A garbage collection error type
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum GcError {}

/// A type that describes allocation of an object into a heap space, returning
/// a bare pointer type on success
// ANCHOR: DefAllocRaw
pub trait AllocRaw {
    /// An implementation of an object header type
    type Header: AllocHeader;

    /// Allocate a single object of type T.
    fn alloc<T>(&self, object: T) -> Result<RawPtr<T>, AllocError>
    where
        T: AllocObject<<Self::Header as AllocHeader>::TypeId>;

    /// Allocating an array allows the client to put anything in the resulting data
    /// block but the type of the memory block will simply be 'Array'. No other
    /// type information will be stored in the object header.
    /// This is just a special case of alloc<T>() for T=u8 but a count > 1 of u8
    /// instances.  The caller is responsible for the content of the array.
    fn alloc_array(&self, size_bytes: ArraySize) -> Result<RawPtr<u8>, AllocError>;

    /// Given a bare pointer to an object, return the expected header address
    fn get_header(object: NonNull<()>) -> NonNull<Self::Header>;

    /// Given a bare pointer to an object's header, return the expected object address
    fn get_object(header: NonNull<Self::Header>) -> NonNull<()>;

    /// Run a garbage collection iteration
    fn gc(&self) -> Result<(), GcError>;
}
// ANCHOR_END: DefAllocRaw

/// Object size class.
/// - Small objects fit inside a line
/// - Medium objects span more than one line
/// - Large objects span multiple blocks
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SizeClass {
    Small,
    Medium,
    Large,
}

impl SizeClass {
    pub fn get_for_size(object_size: usize) -> Result<SizeClass, AllocError> {
        match object_size {
            constants::SMALL_OBJECT_MIN..=constants::SMALL_OBJECT_MAX => Ok(SizeClass::Small),
            constants::MEDIUM_OBJECT_MIN..=constants::MEDIUM_OBJECT_MAX => Ok(SizeClass::Medium),
            constants::LARGE_OBJECT_MIN..=constants::LARGE_OBJECT_MAX => Ok(SizeClass::Large),
            _ => Err(AllocError::BadRequest),
        }
    }
}

/// The type that describes the bounds of array sizing
pub type ArraySize = u32;

/// TODO Object mark bit.
/// Every object is `Allocated` on creation.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Mark {
    Allocated,
    Unmarked,
    Marked,
}

/// A managed-type type-identifier type should implement this!
// ANCHOR: DefAllocTypeId
pub trait AllocTypeId: Copy + Clone {}
// ANCHOR_END: DefAllocTypeId

/// All managed object types must implement this trait in order to be allocatable
// ANCHOR: DefAllocObject
pub trait AllocObject<T: AllocTypeId> {
    const TYPE_ID: T;
}
// ANCHOR_END: DefAllocObject

/// An object header struct must provide an implementation of this trait,
/// providing appropriate information to the garbage collector.
// TODO tracing information
// e.g. fn tracer(&self) -> Fn()
// ANCHOR: DefAllocHeader
pub trait AllocHeader: Sized {
    /// Associated type that identifies the allocated object type
    type TypeId: AllocTypeId;

    /// Create a new header for object type O
    fn new<O: AllocObject<Self::TypeId>>(size: u32, size_class: SizeClass, mark: Mark) -> Self;

    /// Create a new header for an array type
    fn new_array(size: ArraySize, size_class: SizeClass, mark: Mark) -> Self;

    /// Set the Mark value to "marked"
    fn mark(&mut self, value: Mark);

    /// Get the current Mark value
    fn mark_is(&self, value: Mark) -> bool;

    /// Get the size class of the object
    fn size_class(&self) -> SizeClass;

    /// Get the size of the object in bytes
    fn size(&self) -> u32;

    /// Get the type of the object
    fn type_id(&self) -> Self::TypeId;

    /// Get the header size, to the next allocator-aligned number of bytes
    fn header_size() -> usize {
        size_of::<Self>() + (constants::ALLOC_ALIGN_BYTES - 1) & !(constants::ALLOC_ALIGN_BYTES - 1)
    }

    /// Trace into this object. The default behavior is to do nothing.
    /// This is appropriate for objects that do not refer to other objects.
    fn trace(&self) {}

    /// This constant needs to be set to be able to mask out tagged pointer tag bits
    const TAG_MASK: usize = !0x0;
}
// ANCHOR_END: DefAllocHeader
