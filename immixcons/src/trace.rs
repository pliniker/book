use log::trace;

use crate::{RawPtr, TraceVisitor};

/// Structure for collecting pointers across the heap during tracing
pub struct HeapTracer {
    visited: Vec<RawPtr<()>>,
}

impl HeapTracer {
    pub fn new() -> HeapTracer {
        HeapTracer {
            visited: Vec::new(),
        }
    }
}

impl TraceVisitor for HeapTracer {
    fn visit(&mut self, object: RawPtr<()>) {
        trace!("[heap_scan] {:x}", object.addr());
        self.visited.push(object);
    }

    fn pop(&mut self) -> Option<RawPtr<()>> {
        self.visited.pop()
    }
}
