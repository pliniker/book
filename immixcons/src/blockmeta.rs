use crate::constants;

/// Block marking metadata. This metadata is stored at the end of a Block.
// ANCHOR: DefBlockMeta
pub struct BlockMeta {
    lines: *mut u8,
    object_map: *mut u8,
}
// ANCHOR_END: DefBlockMeta

impl BlockMeta {
    /// Heap allocate a metadata instance so that it doesn't move so we can store pointers
    /// to it.
    pub fn new(block_ptr: *const u8) -> BlockMeta {
        let mut meta = BlockMeta {
            lines: unsafe { block_ptr.add(constants::LINE_MARK_START) as *mut u8 },
            object_map: unsafe { block_ptr.add(constants::OBJECT_MAP_START) as *mut u8 },
        };

        meta.reset();

        meta
    }

    unsafe fn as_block_mark(&mut self) -> &mut u8 {
        // Use the last byte of the block because no object will occupy the line
        // associated with this: it's the mark bits.
        &mut *self.lines.add(constants::LINE_COUNT - 1)
    }

    unsafe fn as_line_mark(&mut self, line: usize) -> &mut u8 {
        &mut *self.lines.add(line)
    }

    /// Mark the indexed line
    pub fn mark_line(&mut self, index: usize) {
        unsafe { *self.as_line_mark(index) = 1 };
    }

    /// Indicate the entire block as marked
    pub fn mark_block(&mut self) {
        unsafe { *self.as_block_mark() = 1 }
    }

    /// Reset all mark flags to unmarked.
    pub fn reset(&mut self) {
        unsafe {
            for i in 0..constants::LINE_COUNT {
                *self.lines.add(i) = 0;
            }
            // Reset object map
            for i in 0..constants::OBJECT_MAP_SIZE {
                *self.object_map.add(i) = 0;
            }
        }
    }

    /// Mark an object allocation at the given byte offset within the block.
    /// The offset should be aligned to ALLOC_ALIGN_BYTES.
    pub fn mark_object(&mut self, offset: usize) {
        debug_assert!(offset < constants::ALLOC_UPPER_EXTENT);
        debug_assert!(offset & !constants::ALLOC_ALIGN_MASK == 0);

        let slot = offset / constants::ALLOC_ALIGN_BYTES;
        let byte_index = slot / 8;
        let bit_index = slot % 8;

        unsafe {
            let byte = self.object_map.add(byte_index);
            *byte |= 1 << bit_index;
        }
    }

    /// Check if an object is marked at the given byte offset within the block.
    pub fn is_object_marked(&self, offset: usize) -> bool {
        debug_assert!(offset < constants::ALLOC_UPPER_EXTENT);

        let slot = offset / constants::ALLOC_ALIGN_BYTES;
        let byte_index = slot / 8;
        let bit_index = slot % 8;

        unsafe {
            let byte = *self.object_map.add(byte_index);
            (byte & (1 << bit_index)) != 0
        }
    }

    /// Clear the object mark at the given byte offset within the block.
    pub fn clear_object(&mut self, offset: usize) {
        debug_assert!(offset < constants::BLOCK_CAPACITY);

        let slot = offset / constants::ALLOC_ALIGN_BYTES;
        let byte_index = slot / 8;
        let bit_index = slot % 8;

        unsafe {
            let byte = self.object_map.add(byte_index);
            *byte &= !(1 << bit_index);
        }
    }

    /// Find the next marked object starting from the given offset (inclusive).
    /// Returns the offset of the next marked object, or None if no more objects are marked.
    pub fn find_next_object(&self, starting_offset: usize) -> Option<usize> {
        let start_slot = starting_offset / constants::ALLOC_ALIGN_BYTES;

        for slot in start_slot..constants::OBJECT_MAP_SLOTS {
            let byte_index = slot / 8;
            let bit_index = slot % 8;

            unsafe {
                let byte = *self.object_map.add(byte_index);
                if (byte & (1 << bit_index)) != 0 {
                    return Some(slot * constants::ALLOC_ALIGN_BYTES);
                }
            }
        }

        None
    }

    // Return an iterator over all the line mark flags
    //pub fn line_iter(&self) -> impl Iterator<Item = &'_ bool> {
    //    self.line_mark.iter()
    //}

    // ANCHOR: DefFindNextHole
    /// When it comes to finding allocatable holes, we bump-allocate downward.
    pub fn find_next_available_hole(
        &self,
        starting_at: usize,
        alloc_size: usize,
    ) -> Option<(usize, usize)> {
        // The count of consecutive avaliable holes. Must take into account a conservatively marked
        // hole at the beginning of the sequence.
        let mut count = 0;
        let starting_line = starting_at / constants::LINE_SIZE;
        let lines_required = (alloc_size + constants::LINE_SIZE - 1) / constants::LINE_SIZE;
        // Counting down from the given search start index
        let mut end = starting_line;

        for index in (0..starting_line).rev() {
            let marked = unsafe { *self.lines.add(index) };

            if marked == 0 {
                // count unmarked lines
                count += 1;

                if index == 0 && count >= lines_required {
                    let limit = index * constants::LINE_SIZE;
                    let cursor = end * constants::LINE_SIZE;
                    return Some((cursor, limit));
                }
            } else {
                // This block is marked
                if count > lines_required {
                    // But at least 2 previous blocks were not marked. Return the hole, considering the
                    // immediately preceding block as conservatively marked
                    let limit = (index + 2) * constants::LINE_SIZE;
                    let cursor = end * constants::LINE_SIZE;
                    return Some((cursor, limit));
                }

                // If this line is marked and we didn't return a new cursor/limit pair by now,
                // reset the hole search state
                count = 0;
                end = index;
            }
        }

        None
    }
    // ANCHOR_END: DefFindNextHole
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::blockalloc::Block;

    #[test]
    fn test_find_next_hole() {
        // A set of marked lines with a couple holes.
        // The first hole should be seen as conservatively marked.
        // The second hole should be the one selected.
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let mut meta = BlockMeta::new(block.as_ptr());

        meta.mark_line(0);
        meta.mark_line(1);
        meta.mark_line(2);
        meta.mark_line(4);
        meta.mark_line(10);

        // line 5 should be conservatively marked
        let expect = Some((10 * constants::LINE_SIZE, 6 * constants::LINE_SIZE));

        let got = meta.find_next_available_hole(10 * constants::LINE_SIZE, constants::LINE_SIZE);

        println!("test_find_next_hole got {got:?} expected {expect:?}");

        assert!(got == expect);
    }

    #[test]
    fn test_find_next_hole_at_line_zero() {
        // Should find the hole starting at the beginning of the block
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let mut meta = BlockMeta::new(block.as_ptr());

        meta.mark_line(3);
        meta.mark_line(4);
        meta.mark_line(5);

        let expect = Some((3 * constants::LINE_SIZE, 0));

        let got = meta.find_next_available_hole(3 * constants::LINE_SIZE, constants::LINE_SIZE);

        println!("test_find_next_hole_at_line_zero got {got:?} expected {expect:?}");

        assert!(got == expect);
    }

    #[test]
    fn test_find_next_hole_at_block_end() {
        // The first half of the block is marked.
        // The second half of the block should be identified as a hole.
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let mut meta = BlockMeta::new(block.as_ptr());

        let halfway = constants::LINE_COUNT / 2;

        for i in halfway..constants::LINE_COUNT {
            meta.mark_line(i);
        }

        // because halfway line should be conservatively marked
        let expect = Some((halfway * constants::LINE_SIZE, 0));

        let got = meta.find_next_available_hole(constants::BLOCK_CAPACITY, constants::LINE_SIZE);

        println!("test_find_next_hole_at_block_end got {got:?} expected {expect:?}");

        assert!(got == expect);
    }

    #[test]
    fn test_find_hole_all_conservatively_marked() {
        // Every other line is marked.
        // No hole should be found.
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let mut meta = BlockMeta::new(block.as_ptr());

        for i in 0..constants::LINE_COUNT {
            if i % 2 == 0 {
                // there is no stable step function for range
                meta.mark_line(i);
            }
        }

        let got = meta.find_next_available_hole(constants::BLOCK_CAPACITY, constants::LINE_SIZE);

        println!("test_find_hole_all_conservatively_marked got {got:?} expected None");
        assert!(got.is_none());
    }

    #[test]
    fn test_find_entire_block() {
        // No marked lines. Entire block is available.
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let meta = BlockMeta::new(block.as_ptr());

        let expect = Some((constants::BLOCK_CAPACITY, 0));
        let got = meta.find_next_available_hole(constants::BLOCK_CAPACITY, constants::LINE_SIZE);

        println!("test_find_entire_block got {got:?} expected {expect:?}");

        assert!(got == expect);
    }

    #[test]
    fn test_object_map_mark_and_check() {
        // Test marking and checking individual object slots
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let mut meta = BlockMeta::new(block.as_ptr());

        // Initially, no objects should be marked
        assert!(!meta.is_object_marked(0));
        assert!(!meta.is_object_marked(constants::ALLOC_ALIGN_BYTES));
        assert!(!meta.is_object_marked(constants::ALLOC_ALIGN_BYTES * 2));

        // Mark some objects
        meta.mark_object(0);
        meta.mark_object(constants::ALLOC_ALIGN_BYTES * 5);
        meta.mark_object(constants::ALLOC_ALIGN_BYTES * 100);

        // Check marked objects
        assert!(meta.is_object_marked(0));
        assert!(meta.is_object_marked(constants::ALLOC_ALIGN_BYTES * 5));
        assert!(meta.is_object_marked(constants::ALLOC_ALIGN_BYTES * 100));

        // Check unmarked objects
        assert!(!meta.is_object_marked(constants::ALLOC_ALIGN_BYTES));
        assert!(!meta.is_object_marked(constants::ALLOC_ALIGN_BYTES * 2));
        assert!(!meta.is_object_marked(constants::ALLOC_ALIGN_BYTES * 99));
    }

    #[test]
    fn test_object_map_clear() {
        // Test clearing marked objects
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let mut meta = BlockMeta::new(block.as_ptr());

        let offset = constants::ALLOC_ALIGN_BYTES * 10;

        // Mark an object
        meta.mark_object(offset);
        assert!(meta.is_object_marked(offset));

        // Clear it
        meta.clear_object(offset);
        assert!(!meta.is_object_marked(offset));
    }

    #[test]
    fn test_object_map_find_next() {
        // Test finding marked objects
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let mut meta = BlockMeta::new(block.as_ptr());

        // Mark objects at various positions
        meta.mark_object(constants::ALLOC_ALIGN_BYTES * 5);
        meta.mark_object(constants::ALLOC_ALIGN_BYTES * 10);
        meta.mark_object(constants::ALLOC_ALIGN_BYTES * 50);

        // Find from the beginning
        let first = meta.find_next_object(0);
        assert_eq!(first, Some(constants::ALLOC_ALIGN_BYTES * 5));

        // Find from after the first
        let second = meta.find_next_object(constants::ALLOC_ALIGN_BYTES * 6);
        assert_eq!(second, Some(constants::ALLOC_ALIGN_BYTES * 10));

        // Find from after the second
        let third = meta.find_next_object(constants::ALLOC_ALIGN_BYTES * 11);
        assert_eq!(third, Some(constants::ALLOC_ALIGN_BYTES * 50));

        // Find from after all marked objects
        let none = meta.find_next_object(constants::ALLOC_ALIGN_BYTES * 51);
        assert_eq!(none, None);
    }

    #[test]
    fn test_object_map_reset() {
        // Test that reset clears the object map
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let mut meta = BlockMeta::new(block.as_ptr());

        // Mark several objects
        for i in 0..10 {
            meta.mark_object(constants::ALLOC_ALIGN_BYTES * i);
        }

        // Verify they're marked
        for i in 0..10 {
            assert!(meta.is_object_marked(constants::ALLOC_ALIGN_BYTES * i));
        }

        // Reset
        meta.reset();

        // Verify they're all cleared
        for i in 0..10 {
            assert!(!meta.is_object_marked(constants::ALLOC_ALIGN_BYTES * i));
        }

        // Verify find_next_object returns None
        assert_eq!(meta.find_next_object(0), None);
    }

    #[test]
    fn test_object_map_dense_marking() {
        // Test marking many consecutive objects
        let block = Block::new(constants::BLOCK_SIZE).unwrap();
        let mut meta = BlockMeta::new(block.as_ptr());

        let num_objects = 100;

        // Mark consecutive objects
        for i in 0..num_objects {
            meta.mark_object(constants::ALLOC_ALIGN_BYTES * i);
        }

        // Verify all are marked
        for i in 0..num_objects {
            assert!(meta.is_object_marked(constants::ALLOC_ALIGN_BYTES * i));
        }

        // Verify we can find them all
        let mut current_offset = 0;
        let mut found_count = 0;
        while let Some(offset) = meta.find_next_object(current_offset) {
            assert_eq!(offset, constants::ALLOC_ALIGN_BYTES * found_count);
            found_count += 1;
            current_offset = offset + constants::ALLOC_ALIGN_BYTES;
        }
        assert_eq!(found_count, num_objects);
    }
}
