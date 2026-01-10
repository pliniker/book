/*
 * Block Layout
 *
 * [0 .. 32256)          : Object allocation space (BLOCK_CAPACITY)
 * [32256 .. 32512)      : Line marks (256 bytes, one per line)
 * [32512 .. 32768)      : Object map (256 bytes, bitmap)
 */

// ANCHOR: ConstBlockSize
pub const BLOCK_SIZE_BITS: usize = 15;
pub const BLOCK_SIZE: usize = 1 << BLOCK_SIZE_BITS;
// ANCHOR_END: ConstBlockSize
pub const BLOCK_PTR_MASK: usize = !(BLOCK_SIZE - 1);

// ANCHOR: ConstLineSize
pub const LINE_SIZE_BITS: usize = 7;
pub const LINE_SIZE: usize = 1 << LINE_SIZE_BITS;

// How many total lines are in a block
pub const LINE_COUNT: usize = BLOCK_SIZE / LINE_SIZE;

// Allocation alignment
pub const ALLOC_ALIGN_BYTES: usize = 16;
pub const ALLOC_ALIGN_MASK: usize = !(ALLOC_ALIGN_BYTES - 1);

// Object map for tracking allocated objects
// Each bit represents one ALLOC_ALIGN_BYTES-sized slot
// We need to calculate metadata size first to determine capacity
// Maximum possible slots if entire block was allocatable
const MAX_POSSIBLE_SLOTS: usize = BLOCK_SIZE / ALLOC_ALIGN_BYTES;
// Object map size in bytes (round up to nearest byte)
pub const OBJECT_MAP_SIZE: usize = (MAX_POSSIBLE_SLOTS + 7) / 8;

// Total metadata size: line marks + object map
const METADATA_SIZE: usize = LINE_COUNT + OBJECT_MAP_SIZE;

// We need LINE_COUNT bytes for line marks and OBJECT_MAP_SIZE bytes for object map,
// so the capacity of a block is reduced by the total metadata size.
pub const BLOCK_CAPACITY: usize = BLOCK_SIZE - METADATA_SIZE;
// ANCHOR_END: ConstLineSize

// The first line-mark offset into the block is here.
pub const LINE_MARK_START: usize = BLOCK_CAPACITY;

// Object map starts right after line marks
pub const OBJECT_MAP_START: usize = LINE_MARK_START + LINE_COUNT;

// Actual number of object map slots based on final capacity
pub const OBJECT_MAP_SLOTS: usize = BLOCK_CAPACITY / ALLOC_ALIGN_BYTES;

// Object size ranges
pub const MAX_ALLOC_SIZE: usize = u32::MAX as usize;
pub const SMALL_OBJECT_MIN: usize = 1;
pub const SMALL_OBJECT_MAX: usize = LINE_SIZE;
pub const MEDIUM_OBJECT_MIN: usize = SMALL_OBJECT_MAX + 1;
pub const MEDIUM_OBJECT_MAX: usize = BLOCK_CAPACITY;
pub const LARGE_OBJECT_MIN: usize = MEDIUM_OBJECT_MAX + 1;
pub const LARGE_OBJECT_MAX: usize = MAX_ALLOC_SIZE;
