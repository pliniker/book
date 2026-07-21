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

    pub fn pop_top(&mut self) -> Option<usize> {
        let mut entry = self.gram.last_entry()?;
        let block_list = entry.get_mut();
        let block_id = block_list.pop();

        if block_list.is_empty() {
            entry.remove_entry();
        }

        block_id
    }

    pub fn push_block(&mut self, holes: usize, block: usize) {
        self.gram.entry(holes).or_insert(Vec::new()).push(block);
    }

    pub fn drain_empty_blocks(&mut self) -> std::vec::Drain<'_, usize> {
        self.gram
            .entry(constants::LINE_COUNT)
            .or_default()
            .drain(0..)
    }

    pub fn clear(&mut self) {
        self.gram.clear();
    }
}
