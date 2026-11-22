use std::collections::BTreeMap;

use crate::constants;

type Blocks = Vec<usize>;

/// Block holes histogram
///
/// Maps number of holes to block base address
pub struct Histogram {
    gram: BTreeMap<usize, Blocks>,
}

impl Histogram {
    pub fn new() -> Histogram {
        Histogram {
            gram: BTreeMap::new(),
        }
    }

    /// If the histogram has blocks with at least the minimum required holes,
    /// return the next block in that list.
    pub fn pop_next_recycleable(&mut self) -> Option<usize> {
        let mut entry = self.gram.last_entry()?;

        if *entry.key() > constants::BLOCKS_RECYCLE_MIN_HOLES {
            let block_list = entry.get_mut();
            let block_id = block_list.pop();

            if block_list.is_empty() {
                entry.remove_entry();
            }

            block_id
        } else {
            None
        }
    }

    /// Add a block to the histogram
    pub fn push_block(&mut self, holes: usize, block: usize) {
        self.gram.entry(holes).or_insert(Vec::new()).push(block);
    }

    /// Remove all empty blocks from the histogram
    pub fn drain_empty_blocks(&mut self) -> std::vec::Drain<'_, usize> {
        self.gram
            .entry(constants::LINE_COUNT)
            .or_default()
            .drain(0..)
    }

    /// Wipe out the histogram in order to start again
    pub fn clear(&mut self) {
        self.gram.clear();
    }
}
