use std::collections::BTreeMap;

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
        if let Some(mut entry) = self.gram.last_entry() {
            return entry.get_mut().pop();
        }
        None
    }

    pub fn push_block(&mut self, holes: usize, block: usize) {
        self.gram.entry(holes).or_insert(Vec::new()).push(block);
    }

    pub fn clear(&mut self) {
        self.gram.clear();
    }
}
