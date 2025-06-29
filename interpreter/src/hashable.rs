/// Scope-guard limited Hashable trait type
use std::hash::Hasher;

use crate::safeptr::MutatorScope;

// ANCHOR: DefHashable
/// Similar to Hash but for use in a mutator lifetime-limited scope
pub trait Hashable {
    fn hash<H: Hasher>(&self, _guard: &'_ dyn MutatorScope, hasher: &mut H);
}
// ANCHOR_END: DefHashable
