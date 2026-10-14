use std::num::NonZero;

use crate::constants;

use blockalloc::{Block, BlockPtr};

/// Essentially a copy of a block base pointer
///
/// Not at all really opaque but intended to create type safety around a Block
/// base pointer, making it hard to create invalid instances -  it must be
/// constructed from a Block and cannot be modified.
///
/// This handle is lifetime-tied to an implementation of BlockCollection so that
/// it cannot escaped the scope of a BlockCollection.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockHandle {
    base: BlockPtr,
}

impl<'c> BlockHandle {
    pub fn new(from_block: &Block) -> BlockHandle {
        BlockHandle {
            base: from_block.as_ptr(),
        }
    }

    /// Take ANY pointer and derive a BlockHandle.
    ///
    /// There is no guarantee that the block base pointed at by the new BlockHandle
    /// is a valid Block, thus this is unsafe.
    pub unsafe fn from_any_ptr(ptr: BlockPtr) -> BlockHandle {
        let block_base = ptr.map_addr(|p| {
            let masked_p = p.get() & constants::BLOCK_PTR_MASK;
            unsafe { NonZero::new_unchecked(masked_p) }
        });
        BlockHandle { base: block_base }
    }

    /// Return the block base pointer as is
    pub fn base(&self) -> BlockPtr {
        self.base
    }

    /// This is not considered unsafe because the origin should be a valid
    /// pointer and the offset is bounds checked.
    /// The UB-avoidance conditions of NonNull::offset() should be satisfied.
    ///
    /// Safety of subsequent operations cannot be guaranteed.
    ///
    /// Will panic if the new pointer is outside of Block bounds.
    pub fn offset(&self, count: usize) -> BlockPtr {
        if count >= constants::BLOCK_SIZE {
            panic!("Invalid offset from BlockHandle requested")
        }
        unsafe { self.base.offset(count as isize) }
    }

    /// Get the byte count to a higher pointer
    ///
    /// Will panic if invalid ptr given: see `NonNull::offset_from_unsigned()`
    pub fn byte_count_to(&self, ptr: BlockPtr) -> usize {
        unsafe { ptr.offset_from_unsigned(self.base) }
    }
}

/// A copy of a block base pointer adjusted by the line offset
///
/// This handle is lifetime-tied to an implementation of BlockCollection so that
/// it cannot escaped the scope of a BlockCollection.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LinesHandle {
    lines: BlockPtr,
}

/// A copy of a block base pointer adjusted by the line offset
///
/// This handle is lifetime-tied to an implementation of BlockCollection so that
/// it cannot escaped the scope of a BlockCollection.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObjectMapHandle {
    object_map: BlockPtr,
}
