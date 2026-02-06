use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::mem::size_of;
use std::ptr::{write, NonNull};
use std::slice::from_raw_parts_mut;

use crate::allocator::{
    AllocError, AllocHeader, AllocObject, AllocRaw, ArraySize, GcError, Mark, SizeClass,
};
use crate::blockmeta::BlockMeta;
use crate::bumpblock::BumpBlock;
use crate::constants;
use crate::rawptr::RawPtr;
use crate::stack::SystemStackInfo;
use blockalloc::{Block, BlockError};

impl From<BlockError> for AllocError {
    fn from(error: BlockError) -> AllocError {
        match error {
            BlockError::BadRequest => AllocError::BadRequest,
            BlockError::OOM => AllocError::OOM,
        }
    }
}

/// Return value of finding a space to allocate into
#[derive(Copy, Clone)]
struct AllocDest {
    block: *const u8,
    space: *const u8,
}

impl AllocDest {
    fn new(block: *const u8, space: *const u8) -> AllocDest {
        AllocDest { block, space }
    }
}

/// A list of blocks as the current block being allocated into and a list
/// of full blocks
// TODO:
// free: Vec<usize>,
// recycle: Vec<usize>
// large: Vec<Thing> - large objects will likely be implemented by an
//   indirection via a small object pointer
// ANCHOR: DefBlockList
struct BlockList {
    head: Option<BumpBlock>,
    overflow: Option<BumpBlock>,
    rest: HashMap<usize, Block>,
}
// ANCHOR_END: DefBlockList

impl BlockList {
    fn new() -> BlockList {
        BlockList {
            head: None,
            overflow: None,
            rest: HashMap::new(),
        }
    }

    /// Allocate a space for a medium object into an overflow block
    // TODO this just allocates a new block on demand, but should look at the free block list first
    // ANCHOR: DefOverflowAlloc
    fn overflow_alloc(&mut self, alloc_size: usize) -> Result<AllocDest, AllocError> {
        assert!(alloc_size <= constants::BLOCK_CAPACITY);

        let dest = match self.overflow {
            // We already have an overflow block to try to use...
            Some(ref mut overflow) => {
                // This is a medium object that might fit in the current block...
                match overflow.inner_alloc(alloc_size) {
                    // the block has a suitable hole
                    Some(space) => AllocDest::new(overflow.block_ptr(), space),

                    // the block does not have a suitable hole
                    None => {
                        let block = Block::new(constants::BLOCK_SIZE)?;
                        *overflow = unsafe { BumpBlock::new(block.as_ptr()) };

                        self.rest.insert(block.addr(), block);

                        let space = overflow.inner_alloc(alloc_size).expect("Unexpected error!");
                        AllocDest::new(overflow.block_ptr(), space)
                    }
                }
            }

            // We have no blocks to work with yet so make one
            None => {
                let block = Block::new(constants::BLOCK_SIZE)?;
                let mut overflow = unsafe { BumpBlock::new(block.as_ptr()) };
                let block_ptr = block.as_ptr();

                self.rest.insert(block.addr(), block);

                // earlier check for object size < block size should
                // mean we dont fail this expectation
                let space = overflow
                    .inner_alloc(alloc_size)
                    .expect("We expected this object to fit!");

                self.overflow = Some(overflow);

                AllocDest::new(block_ptr, space)
            }
        };

        Ok(dest)
    }
    // ANCHOR_END: DefOverflowAlloc

    /// Using best effort logic, estimate if a pointer is a valid heap
    /// pointer.
    fn is_conservatively_a_ptr(&self, ptr: usize) -> bool {
        // Regarding the low bits of any word:
        // these bits may be nonzero if they're used for pointer tagging.
        // We have to mask out low bits just to be certain.

        let block_base = ptr & constants::BLOCK_PTR_MASK;
        let block_offset = (ptr & !constants::BLOCK_PTR_MASK) & !0xf;

        if let Some(ref block) = self.rest.get(&block_base) {
            let meta = unsafe { BlockMeta::attach(block.as_ptr()) };
            return block_offset < constants::ALLOC_UPPER_EXTENT
                && meta.is_object_marked(block_offset);
        }

        false
    }
}

/// A type that implements `AllocRaw` to provide a low-level heap interface.
/// Does not allocate internally on initialization.
// ANCHOR: DefStickyImmixHeap
pub struct ImmixConsHeap<H> {
    blocks: UnsafeCell<BlockList>,
    stack: SystemStackInfo,
    _header_type: PhantomData<*const H>,
}
// ANCHOR_END: DefStickyImmixHeap

impl<H> ImmixConsHeap<H> {
    pub fn new() -> ImmixConsHeap<H> {
        ImmixConsHeap {
            blocks: UnsafeCell::new(BlockList::new()),
            stack: SystemStackInfo::new(),
            _header_type: PhantomData,
        }
    }

    /// Find a space for a small, medium or large object
    // TODO this just allocates a new block, but should look at
    // recycled blocks first
    fn find_space(
        &self,
        alloc_size: usize,
        size_class: SizeClass,
    ) -> Result<AllocDest, AllocError> {
        let blocks = unsafe { &mut *self.blocks.get() };

        // TODO handle large objects
        if size_class == SizeClass::Large {
            // simply fail for objects larger than the block size
            return Err(AllocError::BadRequest);
        }

        let dest = match blocks.head {
            // We already have a block to try to use...
            Some(ref mut head) => {
                // If this is a medium object that doesn't fit in the hole, use overflow
                if size_class == SizeClass::Medium && alloc_size > head.current_hole_size() {
                    return blocks.overflow_alloc(alloc_size);
                }

                // This is a small object that might fit in the current block...
                match head.inner_alloc(alloc_size) {
                    // the block has a suitable hole
                    Some(space) => AllocDest::new(head.block_ptr(), space),

                    // the block does not have a suitable hole so allocate a new head block
                    None => {
                        let block = Block::new(constants::BLOCK_SIZE)?;
                        *head = unsafe { BumpBlock::new(block.as_ptr()) };

                        blocks.rest.insert(block.addr(), block);

                        let space = head.inner_alloc(alloc_size).expect("Unexpected error!");
                        AllocDest::new(head.block_ptr(), space)
                    }
                }
            }

            // We have no blocks to work with yet so make one
            None => {
                let block = Block::new(constants::BLOCK_SIZE)?;
                let mut head = unsafe { BumpBlock::new(block.as_ptr()) };
                let block_ptr = block.as_ptr();

                blocks.rest.insert(block.addr(), block);

                // earlier check for object size < block size should
                // mean we dont fail this expectation
                let space = head
                    .inner_alloc(alloc_size)
                    .expect("We expected this object to fit!");

                blocks.head = Some(head);

                AllocDest::new(block_ptr, space)
            }
        };

        Ok(dest)
    }
}

impl<H: AllocHeader> AllocRaw for ImmixConsHeap<H> {
    type Header = H;

    /// Allocate space for object `T`, creating an header for it and writing the object
    /// and the header into the space
    // ANCHOR: DefAlloc
    fn alloc<T>(&self, object: T) -> Result<RawPtr<T>, AllocError>
    where
        T: AllocObject<<Self::Header as AllocHeader>::TypeId>,
    {
        // calculate the total size of the object and it's header
        let header_size = Self::Header::header_size();
        let object_size = size_of::<T>();
        let total_size = header_size + object_size;

        // round the size to the next word boundary to keep objects aligned and get the size class
        let size_class = SizeClass::get_for_size(total_size)?;

        // attempt to allocate enough space for the header and the object
        let dest = self.find_space(total_size, size_class)?;

        // instantiate an object header for type T, setting the mark bit to "allocated"
        let header = Self::Header::new::<T>(object_size as ArraySize, size_class, Mark::Allocated);

        // write the header into the front of the allocated space
        unsafe {
            write(dest.space as *mut Self::Header, header);
        }

        // write the object into the allocated space after the header
        let object_space = unsafe { dest.space.add(header_size) };
        unsafe {
            write(object_space as *mut T, object);
        }

        // Mark this object in the block's object map
        let object_offset = object_space as usize - dest.block as usize;
        unsafe {
            let mut meta = BlockMeta::attach(dest.block);
            meta.mark_object(object_offset);
        }

        // return a pointer to the object in the allocated space
        Ok(RawPtr::new(object_space as *const T))
    }
    // ANCHOR_END: DefAlloc

    /// Allocate space for an array, creating an header for it, writing the header into the space
    /// and returning a pointer to the array space
    // ANCHOR: DefAllocArray
    fn alloc_array(&self, size_bytes: ArraySize) -> Result<RawPtr<u8>, AllocError> {
        // calculate the total size of the array and its header
        let header_size = Self::Header::header_size();
        let total_size = header_size + size_bytes as usize;

        // round the size to the next word boundary to keep objects aligned and get the size class
        let size_class = SizeClass::get_for_size(total_size)?;

        // attempt to allocate enough space for the header and the array
        let dest = self.find_space(total_size, size_class)?;

        // instantiate an object header for an array, setting the mark bit to "allocated"
        let header = Self::Header::new_array(size_bytes, size_class, Mark::Allocated);

        // write the header into the front of the allocated space
        unsafe {
            write(dest.space as *mut Self::Header, header);
        }

        // calculate where the array will begin after the padded header
        let array_space = unsafe { dest.space.add(header_size) };

        // Initialize array to zero
        let array = unsafe { from_raw_parts_mut(array_space as *mut u8, size_bytes as usize) };
        for byte in array {
            *byte = 0;
        }

        // Mark this object in the block's object map
        let object_offset = array_space as usize - dest.block as usize;
        unsafe {
            let mut meta = BlockMeta::attach(dest.block);
            meta.mark_object(object_offset);
        }

        Ok(RawPtr::new(array_space))
    }
    // ANCHOR_END: DefAllocArray

    /// Return the object header for a given object pointer
    // ANCHOR: DefGetHeader
    fn get_header(object: NonNull<()>) -> NonNull<Self::Header> {
        let padded_header = Self::Header::header_size();
        unsafe {
            let header_ptr = (object.as_ptr() as *const u8).sub(padded_header) as *mut Self::Header;
            NonNull::new_unchecked(header_ptr)
        }
    }
    // ANCHOR_END: DefGetHeader

    /// Return the object from it's header address
    // ANCHOR: DefGetObject
    fn get_object(header: NonNull<Self::Header>) -> NonNull<()> {
        let padded_header = Self::Header::header_size();
        unsafe {
            let obj_ptr = (header.as_ptr() as *const u8).add(padded_header) as *mut ();
            NonNull::new_unchecked(obj_ptr)
        }
    }
    // ANCHOR_END: DefGetObject

    /// Run a garbage collection iteration
    fn gc(&self) -> Result<(), GcError> {
        let blocks = unsafe { &mut *self.blocks.get() };

        // 1. stack scan for things that could be pointers into the heap
        let mut stack_scan = Vec::new();
        self.stack.scan(&mut stack_scan, |ptr| {
            blocks.is_conservatively_a_ptr(ptr & Self::Header::tag_mask)
        });

        // 2. trace
        // 3. collect
        // 4. manage blocks

        Ok(())
    }
}

impl<H> Default for ImmixConsHeap<H> {
    fn default() -> ImmixConsHeap<H> {
        ImmixConsHeap::new()
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::allocator::{AllocObject, AllocTypeId, Mark, SizeClass};
    use std::slice::from_raw_parts;

    struct TestHeader {
        _size_class: SizeClass,
        _mark: Mark,
        type_id: TestTypeId,
        _size_bytes: u32,
    }

    #[derive(PartialEq, Copy, Clone)]
    enum TestTypeId {
        Biggish,
        Stringish,
        Usizeish,
        Array,
    }

    impl AllocTypeId for TestTypeId {}

    impl AllocHeader for TestHeader {
        type TypeId = TestTypeId;

        fn new<O: AllocObject<Self::TypeId>>(size: u32, size_class: SizeClass, mark: Mark) -> Self {
            TestHeader {
                _size_class: size_class,
                _mark: mark,
                type_id: O::TYPE_ID,
                _size_bytes: size,
            }
        }

        fn new_array(size: u32, size_class: SizeClass, mark: Mark) -> Self {
            TestHeader {
                _size_class: size_class,
                _mark: mark,
                type_id: TestTypeId::Array,
                _size_bytes: size,
            }
        }

        fn mark(&mut self, _value: Mark) {}

        fn mark_is(&self, _value: Mark) -> bool {
            true
        }

        fn size_class(&self) -> SizeClass {
            SizeClass::Small
        }

        fn size(&self) -> u32 {
            8
        }

        fn type_id(&self) -> TestTypeId {
            self.type_id
        }
    }

    struct Big {
        _huge: [u8; constants::BLOCK_SIZE + 1],
    }

    impl Big {
        fn make() -> Big {
            Big {
                _huge: [0u8; constants::BLOCK_SIZE + 1],
            }
        }
    }

    impl AllocObject<TestTypeId> for Big {
        const TYPE_ID: TestTypeId = TestTypeId::Biggish;
    }

    impl AllocObject<TestTypeId> for String {
        const TYPE_ID: TestTypeId = TestTypeId::Stringish;
    }

    impl AllocObject<TestTypeId> for usize {
        const TYPE_ID: TestTypeId = TestTypeId::Usizeish;
    }

    #[test]
    fn test_memory() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        match mem.alloc(String::from("foo")) {
            Ok(s) => {
                let orig = unsafe { s.as_ref() };
                assert!(*orig == String::from("foo"));
            }

            Err(_) => panic!("Allocation failed"),
        }
    }

    #[test]
    fn test_too_big() {
        let mem = ImmixConsHeap::<TestHeader>::new();
        assert!(mem.alloc(Big::make()) == Err(AllocError::BadRequest));
    }

    #[test]
    fn test_many_obs() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        let mut obs = Vec::new();

        // allocate a sequence of numbers
        for i in 0..(constants::BLOCK_SIZE * 3) {
            match mem.alloc(i) {
                Err(_) => panic!("Allocation failed unexpectedly"),
                Ok(ptr) => obs.push(ptr),
            }
        }

        // check that all values of allocated words match the original
        // numbers written, that no heap corruption occurred
        for (i, ob) in obs.iter().enumerate() {
            assert!(i == unsafe { *ob.as_ref() })
        }
    }

    #[test]
    fn test_array() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        let size = 2048;

        match mem.alloc_array(size) {
            Err(_) => panic!("Array allocation failed unexpectedly"),

            Ok(ptr) => {
                // Validate that array is zero initialized all the way through
                let ptr = ptr.as_ptr();

                let array = unsafe { from_raw_parts(ptr, size as usize) };

                for byte in array {
                    assert!(*byte == 0);
                }
            }
        }
    }

    #[test]
    fn test_alignment() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        match mem.alloc(String::from("foo")) {
            Ok(s) => {
                let untyped_ptr = s.as_untyped();
                let header_ptr = ImmixConsHeap::<TestHeader>::get_header(untyped_ptr);
                let header_addr = header_ptr.as_ptr() as usize;
                let obj_addr = untyped_ptr.as_ptr() as usize;

                assert!(header_addr & (constants::ALLOC_ALIGN_BYTES - 1) == 0);
                assert!(obj_addr & (constants::ALLOC_ALIGN_BYTES - 1) == 0);

                let obj_from_header = ImmixConsHeap::<TestHeader>::get_object(header_ptr);
                assert!(obj_from_header.as_ptr() as usize == obj_addr);
            }

            Err(_) => panic!("Allocation failed"),
        }
    }

    #[test]
    fn test_object_map_marked_on_alloc() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        match mem.alloc(42usize) {
            Ok(ptr) => {
                let untyped_ptr = ptr.as_untyped();
                let block_base = (untyped_ptr.as_ptr() as usize) & constants::BLOCK_PTR_MASK;
                let object_offset = untyped_ptr.as_ptr() as usize - block_base;
                let meta = unsafe { BlockMeta::attach(block_base as *const u8) };
                assert!(meta.is_object_marked(object_offset));
            }

            Err(_) => panic!("Allocation failed"),
        }
    }

    #[test]
    fn test_header() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        match mem.alloc(String::from("foo")) {
            Ok(s) => {
                let untyped_ptr = s.as_untyped();
                let header_ptr = ImmixConsHeap::<TestHeader>::get_header(untyped_ptr);
                dbg!(header_ptr);
                let header = unsafe { &*header_ptr.as_ptr() as &TestHeader };

                assert!(header.type_id() == TestTypeId::Stringish);
            }

            Err(_) => panic!("Allocation failed"),
        }
    }
}
