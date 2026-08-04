use std::ptr::write;

use crate::blockmeta::BlockMeta;
use crate::constants;

/// Safe abstraction around allocating into a block: maintaining a write
/// cursor and allocation extents; includes the lines and object map
/// abstration. Internally this is all raw pointers.
/// This object does not OWN a block, is merely a temporary scaffolding
/// around a current block.
// ANCHOR: DefBumpBlock
pub struct BumpBlock {
    cursor: *const u8,
    limit: *const u8,
    block: *const u8,
}
// ANCHOR_END: DefBumpBlock

impl BumpBlock {
    /// Start working with a new block, wiping lines and object map clean
    pub unsafe fn new(block: *const u8) -> BumpBlock {
        unsafe { BlockMeta::attach_and_reset(block) };
        BumpBlock {
            cursor: unsafe { block.add(constants::BLOCK_CAPACITY) },
            limit: block,
            block: block,
        }
    }

    /// Attach to an existing Block, making no modifications
    pub unsafe fn attach(block: *const u8) -> BumpBlock {
        BumpBlock {
            cursor: unsafe { block.add(constants::BLOCK_CAPACITY) },
            limit: block,
            block: block,
        }
    }

    /// Write an object into the block at the given offset. The offset is not
    /// checked for overflow, hence this function is unsafe.
    unsafe fn write<T>(&mut self, object: T, offset: usize) -> *const T {
        unsafe {
            let p = self.block.add(offset) as *mut T;
            write(p, object);
            p
        }
    }

    /// Find a hole of at least the requested size and return Some(pointer) to it, or
    /// None if this block doesn't have a big enough hole.
    // ANCHOR: DefBumpBlockAlloc
    pub fn inner_alloc(&mut self, alloc_size: usize) -> Option<*const u8> {
        let ptr = self.cursor as usize;
        let limit = self.limit as usize;

        let next_ptr = ptr.checked_sub(alloc_size)? & constants::ALLOC_ALIGN_MASK;

        if next_ptr < limit {
            let block_relative_limit = (self.limit as usize) - (self.block as usize);

            if block_relative_limit > 0 {
                let meta = unsafe { BlockMeta::attach(self.block) };
                if let Some((cursor, limit)) =
                    meta.find_next_available_hole(block_relative_limit, alloc_size)
                {
                    self.cursor = unsafe { self.block.add(cursor) };
                    self.limit = unsafe { self.block.add(limit) };
                    return self.inner_alloc(alloc_size);
                }
            }

            None
        } else {
            self.cursor = next_ptr as *const u8;
            Some(self.cursor)
        }
    }
    // ANCHOR_END: DefBumpBlockAlloc

    /// Return the size of the hole we're positioned at
    pub fn current_hole_size(&self) -> usize {
        self.cursor as usize - self.limit as usize
    }

    /// Return the block pointer we're working with
    pub fn block_ptr(&self) -> *const u8 {
        self.block
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
            let u32ptr = ptr as *mut u32;

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
        let mut b = unsafe { BumpBlock::new(block.as_ptr()) };

        let count = loop_check_allocate(&mut b);
        let expect = constants::BLOCK_CAPACITY / TEST_UNIT_SIZE;

        println!("expect={expect}, count={count}");
        assert!(count == expect);
    }

    #[test]
    fn test_half_block() {
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        // This block has an available hole as the second half of the block
        let mut b = unsafe { BumpBlock::new(block.as_ptr()) };
        let mut meta = unsafe { BlockMeta::attach_and_reset(block.as_ptr()) };

        for i in 0..(constants::LINE_COUNT / 2) {
            meta.mark_line(i);
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
        let mut b = unsafe { BumpBlock::new(block.as_ptr()) };
        let mut meta = unsafe { BlockMeta::attach_and_reset(block.as_ptr()) };

        for i in 0..constants::LINE_COUNT {
            if i % 2 == 0 {
                meta.mark_line(i);
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

        // Initialize metadata and mark a line
        let mut meta = unsafe { BlockMeta::attach_and_reset(block.as_ptr()) };
        meta.mark_line(3);

        // Attach to the block (should not reset metadata)
        let b = unsafe { BumpBlock::attach(block.as_ptr()) };

        // block pointer should be correct
        assert_eq!(b.block_ptr(), block.as_ptr());

        // initial hole should be the entire block
        assert_eq!(b.current_hole_size(), constants::BLOCK_CAPACITY);

        // verify the line mark we set earlier is still present
        let raw = block.as_ptr();
        let line_mark = unsafe { *raw.add(constants::LINE_MARK_START + 3) };
        assert_eq!(line_mark, 1);
    }

    #[test]
    fn test_write_writes_at_offset() {
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let mut b = unsafe { BumpBlock::new(block.as_ptr()) };

        // Write a u32 at offset 0
        unsafe {
            let p = b.write::<u32>(0xDEADBEEF_u32, 0);
            assert_eq!(*p, 0xDEADBEEF_u32);
        }

        // Write another value at a different offset
        let offset = constants::ALLOC_ALIGN_BYTES * 2;
        unsafe {
            let p2 = b.write::<u32>(0xCAFEBABE_u32, offset);
            assert_eq!(*p2, 0xCAFEBABE_u32);
        }
    }

    #[test]
    fn test_attach_write_monomorphization() {
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let mut b = unsafe { BumpBlock::attach(block.as_ptr()) };

        unsafe {
            // u32 write via attached bump block
            let p1 = b.write::<u32>(0xAABBCCDD_u32, 8);
            assert_eq!(*p1, 0xAABBCCDD_u32);

            // u64 write via attached bump block to force another monomorphization
            let p2 = b.write::<u64>(0xDEADBEEFDEADBEEF_u64, 16);
            assert_eq!(*p2, 0xDEADBEEFDEADBEEF_u64);

            // write a small struct to exercise a third instantiation
            #[derive(PartialEq, Debug)]
            struct S {
                a: u32,
                b: u32,
            }

            let s = S { a: 1, b: 2 };
            let ps = b.write::<S>(S { a: 1, b: 2 }, 32);
            assert_eq!(*ps, s);
        }
    }
}
