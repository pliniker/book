use std::num::NonZero;
use std::ptr::write;

use blockalloc::BlockPtr;

use crate::blockmeta::BlockMeta;
use crate::constants;
use crate::types::BlockHandle;

/// Safe abstraction around allocating into a block: maintaining a write
/// cursor and allocation extents; includes the lines and object map
/// abstration. Internally this is all raw pointers.
/// This object does not OWN a block, is merely a temporary scaffolding
/// around a current block.
// ANCHOR: DefBumpBlock
pub struct BumpBlock {
    block: BlockHandle,
    cursor: BlockPtr,
    limit: BlockPtr,
    meta: BlockMeta,
}
// ANCHOR_END: DefBumpBlock

impl BumpBlock {
    /// Start working with a new block, wiping lines and object map clean
    pub fn new(block: BlockHandle) -> BumpBlock {
        let mut meta = block.get_meta();
        meta.reset();

        let cursor = block.offset(constants::BLOCK_CAPACITY);
        let limit = block.base();
        BumpBlock {
            block: block,
            cursor: cursor,
            limit: limit,
            meta: meta,
        }
    }

    /// Attach to an existing Block, making no modifications
    pub fn attach(block: BlockHandle) -> BumpBlock {
        let meta = block.get_meta();
        let cursor = block.offset(constants::BLOCK_CAPACITY);
        let limit = block.base();
        BumpBlock {
            block: block,
            cursor: cursor,
            limit: limit,
            meta: meta,
        }
    }

    /// Get the metadata pointer
    pub fn _get_meta(&self) -> BlockMeta {
        self.block.get_meta()
    }

    /// Write an object into the block at the given offset. The offset is not
    /// checked for overflow, hence this function is unsafe.
    unsafe fn _write<T>(&mut self, object: T, offset: usize) -> *const T {
        let p = self.block.offset(offset).as_ptr() as *mut T;
        unsafe {
            write(p, object);
        }
        p
    }

    /// Find a hole of at least the requested size and return Some(pointer) to it, or
    /// None if this block doesn't have a big enough hole.
    // ANCHOR: DefBumpBlockAlloc
    pub fn inner_alloc(&mut self, alloc_size: usize) -> Option<BlockPtr> {
        let ptr = self.cursor;
        let limit = self.limit;

        let next_ptr = unsafe {
            ptr.sub(alloc_size).map_addr(|p| {
                let masked_p = p.get() & constants::ALLOC_ALIGN_MASK;
                NonZero::new_unchecked(masked_p)
            })
        };

        if next_ptr < limit {
            let block_relative_limit = self.block.byte_count_to(self.limit);

            if block_relative_limit > 0 {
                if let Some((cursor, limit)) = self
                    .meta
                    .find_next_available_hole(block_relative_limit, alloc_size)
                {
                    self.cursor = self.block.offset(cursor);
                    self.limit = self.block.offset(limit);
                    return self.inner_alloc(alloc_size);
                }
            }

            None
        } else {
            self.cursor = next_ptr;
            Some(self.cursor)
        }
    }
    // ANCHOR_END: DefBumpBlockAlloc

    /// Mark the object map - this is done on object allocation, hence this
    /// function is here on the bump allocation cursor.
    pub fn mark_object_map(&mut self, header_size: usize, header: BlockPtr) {
        let header_offset = self.block.byte_count_to(header);
        let object_offset = header_offset + header_size;
        self.meta.mark_object(object_offset);
    }

    /// Return the size of the hole we're positioned at
    pub fn current_hole_size(&self) -> usize {
        unsafe { self.cursor.offset_from_unsigned(self.limit) }
    }

    /// Return the block pointer we're working with
    pub fn _block_ptr(&self) -> BlockPtr {
        self.block.base()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use blockalloc::Block;

    const TEST_UNIT_SIZE: usize = constants::ALLOC_ALIGN_BYTES;

    // Helper function: given the Block, fill all holes with u32 values
    // and return the number of values allocated.
    // Also assert that all allocated values are unchanged as allocation
    // proceeds.
    fn loop_check_allocate(b: &mut BumpBlock) -> usize {
        let mut v = Vec::new();
        let mut index = 0;

        while let Some(ptr) = b.inner_alloc(TEST_UNIT_SIZE) {
            let u32ptr = ptr.as_ptr() as *mut u32;

            assert!(!v.contains(&u32ptr));

            v.push(u32ptr);
            unsafe { *u32ptr = index }

            index += 1;
        }

        for (index, u32ptr) in v.iter().enumerate() {
            unsafe {
                assert!(**u32ptr == index as u32);
            }
        }

        index as usize
    }

    #[test]
    fn test_empty_block() {
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let handle = BlockHandle::new(&block);
        let mut b = BumpBlock::new(handle);

        let count = loop_check_allocate(&mut b);
        let expect = constants::BLOCK_CAPACITY / TEST_UNIT_SIZE;

        println!("expect={expect}, count={count}");
        assert!(count == expect);
    }

    #[test]
    fn test_half_block() {
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let handle = BlockHandle::new(&block);
        // This block has an available hole as the second half of the block
        let mut b = BumpBlock::new(handle);

        for i in 0..(constants::LINE_COUNT / 2) {
            b.meta.mark_line(i);
        }
        let occupied_bytes = (constants::LINE_COUNT / 2) * constants::LINE_SIZE;

        b.limit = b.cursor; // block is recycled

        let count = loop_check_allocate(&mut b);
        let expect =
            (constants::BLOCK_CAPACITY - constants::LINE_SIZE - occupied_bytes) / TEST_UNIT_SIZE;

        println!("expect={expect}, count={count}");
        assert!(count == expect);
    }

    #[test]
    fn test_conservatively_marked_block() {
        // This block has every other line marked, so the alternate lines are conservatively
        // marked. Nothing should be allocated in this block.

        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let handle = BlockHandle::new(&block);
        let mut b = BumpBlock::new(handle);

        for i in 0..constants::LINE_COUNT {
            if i % 2 == 0 {
                b.meta.mark_line(i);
            }
        }

        b.limit = b.cursor; // block is recycled

        let count = loop_check_allocate(&mut b);

        println!("count={count}");
        assert!(count == 0);
    }

    #[test]
    fn test_attach_preserves_metadata_and_block_properties() {
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let handle = BlockHandle::new(&block);

        // Initialize metadata and mark a line
        let mut meta = handle.get_meta();
        meta.mark_line(3);

        // Attach to the block (should not reset metadata)
        let b = handle.attach_bumpcursor();

        // block pointer should be correct
        assert_eq!(b._block_ptr(), block.as_ptr());

        // initial hole should be the entire block
        assert_eq!(b.current_hole_size(), constants::BLOCK_CAPACITY);

        // verify the line mark we set earlier is still present
        let raw = block.as_ptr();
        let line_mark = unsafe { *raw.add(constants::LINE_MARK_START + 3).as_ref() };
        assert_eq!(line_mark, 1);
    }

    #[test]
    fn test_write_writes_at_offset() {
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let handle = BlockHandle::new(&block);
        let mut b = BumpBlock::new(handle);

        // Write a u32 at offset 0
        unsafe {
            let p = b._write::<u32>(0xDEADBEEF_u32, 0);
            assert_eq!(*p, 0xDEADBEEF_u32);
        }

        // Write another value at a different offset
        let offset = constants::ALLOC_ALIGN_BYTES * 2;
        unsafe {
            let p2 = b._write::<u32>(0xCAFEBABE_u32, offset);
            assert_eq!(*p2, 0xCAFEBABE_u32);
        }
    }

    #[test]
    fn test_attach_write_monomorphization() {
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let handle = BlockHandle::new(&block);
        let mut b = BumpBlock::attach(handle);

        unsafe {
            // u32 write via attached bump block
            let p1 = b._write::<u32>(0xAABBCCDD_u32, 8);
            assert_eq!(*p1, 0xAABBCCDD_u32);

            // u64 write via attached bump block to force another monomorphization
            let p2 = b._write::<u64>(0xDEADBEEFDEADBEEF_u64, 16);
            assert_eq!(*p2, 0xDEADBEEFDEADBEEF_u64);

            // write a small struct to exercise a third instantiation
            #[derive(PartialEq, Debug)]
            struct S {
                a: u32,
                b: u32,
            }

            let s = S { a: 1, b: 2 };
            let ps = b._write::<S>(S { a: 1, b: 2 }, 32);
            assert_eq!(*ps, s);
        }
    }
}
