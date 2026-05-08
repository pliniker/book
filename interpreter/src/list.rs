/// List is an Array type that can contain any other object
use crate::array::Array;
use crate::safeptr::TaggedCellPtr;
use crate::trace::Trace;

/// A List can contain a mixed sequence of any type of value
pub type List = Array<TaggedCellPtr>;

impl Trace for List {
    fn trace<V: immixcons::TraceVisitor>(
        &self,
        v: &mut V,
        guard: &'_ dyn crate::safeptr::MutatorScope,
    ) {
        self.inner_trace(v, guard);

        // Safety note
        // -----------
        // Given that Array<T> is single-thread only:
        // At the time of borrowing this as a slice, we should not need to be
        // concerned about mutations under our feet. We are reading each
        // pointer only.
        let slice = unsafe { self.as_slice(guard) };

        for ptr in slice {
            ptr.trace(v, guard);
        }
    }
}
