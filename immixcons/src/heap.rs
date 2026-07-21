use log::trace;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::mem::size_of;
use std::ptr::write;
use std::slice::from_raw_parts_mut;

use crate::allocator::{
    AllocError, AllocHeader, AllocObject, AllocRaw, ArraySize, Mark, SizeClass, TraceVisitor,
};
use crate::blockmeta::BlockMeta;
use crate::bumpblock::BumpBlock;
use crate::constants;
use crate::histogram::Histogram;
use crate::rawptr::RawPtr;
use crate::stack::SystemStackInfo;
use blockalloc::{Block, BlockError};

impl From<BlockError> for AllocError {
    fn from(error: BlockError) -> AllocError {
        match error {
            BlockError::BadRequest => AllocError::BadRequest,
            BlockError::OOM => AllocError::OOM,
        }
    }
}

/// Return value of finding a space to allocate into
#[derive(Copy, Clone)]
struct AllocDest {
    block: *const u8,
    space: *const u8,
}

impl AllocDest {
    fn new(block: *const u8, space: *const u8) -> AllocDest {
        AllocDest { block, space }
    }
}

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

/// A list of blocks as the current block being allocated into and a list
/// of full blocks
// TODO:
//  - large objects
// ANCHOR: DefBlockList
struct BlockList {
    head: Option<BumpBlock>,
    overflow: Option<BumpBlock>,
    empty_blocks: Vec<usize>,
    all_blocks: HashMap<usize, Block>,
    histogram: Histogram,
}
// ANCHOR_END: DefBlockList

impl BlockList {
    fn new() -> BlockList {
        BlockList {
            head: None,
            overflow: None,
            empty_blocks: Vec::new(),
            all_blocks: HashMap::new(),
            histogram: Histogram::new(),
        }
    }

    // Allocate a block and add it to the storage bank
    fn get_new_block(&mut self) -> Result<BumpBlock, AllocError> {
        let block = Block::new(constants::BLOCK_SIZE)?;
        let bumpblock = unsafe { BumpBlock::new(block.as_ptr()) };
        self.all_blocks.insert(block.addr(), block);
        Ok(bumpblock)
    }

    // Get an empty block, falling back to allocating a new block if there are no
    // empty blocks
    fn pop_empty_block(&mut self) -> Result<BumpBlock, AllocError> {
        if let Some(block_base) = self.empty_blocks.pop() {
            if let Some(block) = self.all_blocks.get(&block_base) {
                return Ok(unsafe { BumpBlock::new(block.as_ptr()) });
            }
        }

        // fall back on allocator
        self.get_new_block()
    }

    // Get a recycled block, falling back to the empty block list if no suitable
    // block exists
    fn pop_recycled_block(&mut self) -> Result<BumpBlock, AllocError> {
        // TODO
        // Look for histogram-managed blocks with at least x holes
        unimplemented!()
    }

    /// Manage empty blocks and recycling blocks
    fn manage_blocks(&mut self) -> Result<(), AllocError> {
        // generate histogram
        self.histogram.clear();

        for (block_addr, block) in self.all_blocks.iter() {
            let meta = unsafe { BlockMeta::attach(block.as_ptr()) };
            let holes = meta.count_holes();
            self.histogram.push_block(holes, *block_addr);
        }

        // parse histogram:
        //  - move empty blocks to empty_blocks list
        for block_addr in self.histogram.drain_empty_blocks() {
            self.empty_blocks.push(block_addr);
        }

        // check empty block count, release surplus or claim additional
        let empty_count = self.empty_blocks.len();
        if empty_count > constants::BLOCKS_KEEP_EMPTY_COUNT {
            let take = empty_count - constants::BLOCKS_KEEP_EMPTY_COUNT;
            self.empty_blocks.drain(0..take).for_each(|block_ptr| {
                self.all_blocks.remove(&block_ptr);
            });
        } else if empty_count < constants::BLOCKS_KEEP_EMPTY_COUNT {
            let additional = constants::BLOCKS_KEEP_EMPTY_COUNT - empty_count;
            for _ in 0..additional {
                let block = Block::new(constants::BLOCK_SIZE)?;
                let block_ptr = block.addr();
                self.all_blocks.insert(block_ptr, block);
                self.empty_blocks.push(block_ptr);
            }
        }

        Ok(())
    }

    fn reset_mark_bits(&mut self) {
        // TODO
        // 1. Empty blocks: clear object map
        // 2. Other blocks:
        //   - clear block mark bit
        //   - for unmarked lines, clear object map bits
        //   - for marked lines:
        //     - clear line mark
        //     - for objects in line, clear object mark bit
    }

    /// Allocate a space for a medium object into an overflow block
    // ANCHOR: DefOverflowAlloc
    fn find_overflow_space(&mut self, alloc_size: usize) -> Result<AllocDest, AllocError> {
        assert!(alloc_size <= constants::BLOCK_CAPACITY);

        // Take the current overflow block out so we can borrow `self` again
        // below without a conflict (the old `ref mut overflow` kept a borrow
        // of `self` alive across the `pop_empty_block` call).
        let mut overflow = match self.overflow.take() {
            // We already have an overflow block to try to use...
            Some(overflow) => overflow,

            // We have no blocks to work with yet so make one
            None => {
                let mut overflow = self.pop_empty_block()?;
                let block_ptr = overflow.block_ptr();

                // earlier check for object size < block size should
                // mean we dont fail this expectation
                let space = overflow
                    .inner_alloc(alloc_size)
                    .expect("We expected this object to fit!");

                self.overflow = Some(overflow);

                return Ok(AllocDest::new(block_ptr, space));
            }
        };

        let dest = match overflow.inner_alloc(alloc_size) {
            // the block has a suitable hole
            Some(space) => AllocDest::new(overflow.block_ptr(), space),

            // the block does not have a suitable hole
            None => {
                let new_overflow = self.pop_empty_block()?;
                overflow = new_overflow;

                let space = overflow.inner_alloc(alloc_size).expect("Unexpected error!");
                AllocDest::new(overflow.block_ptr(), space)
            }
        };

        self.overflow = Some(overflow);
        Ok(dest)
    }
    // ANCHOR_END: DefOverflowAlloc
    /// Find a space for a small, medium or large object

    // TODO this just allocates a new block, but should look at
    // recycled blocks first
    fn find_space(
        &mut self,
        alloc_size: usize,
        size_class: SizeClass,
    ) -> Result<AllocDest, AllocError> {
        // TODO handle large objects
        if size_class == SizeClass::Large {
            // simply fail for objects larger than the block size
            return Err(AllocError::BadRequest);
        }

        let dest = match self.head {
            // We already have a block to try to use...
            Some(ref mut head) => {
                // If this is a medium object that doesn't fit in the hole, use overflow
                if size_class == SizeClass::Medium && alloc_size > head.current_hole_size() {
                    return self.find_overflow_space(alloc_size);
                }

                // This is a small object that might fit in the current block...
                match head.inner_alloc(alloc_size) {
                    // the block has a suitable hole
                    Some(space) => AllocDest::new(head.block_ptr(), space),

                    // the block does not have a suitable hole so allocate a new head block
                    None => {
                        // TODO use pop_recycled_block()
                        let block = Block::new(constants::BLOCK_SIZE)?;
                        *head = unsafe { BumpBlock::new(block.as_ptr()) };

                        self.all_blocks.insert(block.addr(), block);

                        let space = head.inner_alloc(alloc_size).expect("Unexpected error!");
                        AllocDest::new(head.block_ptr(), space)
                    }
                }
            }

            // We have no blocks to work with yet so make one
            None => {
                // TODO use pop_recycled_block()
                let block = Block::new(constants::BLOCK_SIZE)?;
                let mut head = unsafe { BumpBlock::new(block.as_ptr()) };
                let block_ptr = block.as_ptr();

                self.all_blocks.insert(block.addr(), block);

                // earlier check for object size < block size should
                // mean we dont fail this expectation
                let space = head
                    .inner_alloc(alloc_size)
                    .expect("We expected this object to fit!");

                self.head = Some(head);

                AllocDest::new(block_ptr, space)
            }
        };

        Ok(dest)
    }

    /// Using best effort logic, estimate if a pointer is a valid heap pointer.
    ///
    /// 1. Masking the pointer to get a potential block base, check that the
    ///    base exists in the block list
    /// 2. Masking the pointer to get a potential block offset, check that the
    ///    offset is marked in the block's object map
    #[inline(always)]
    fn is_conservatively_a_ptr(&self, ptr: usize) -> Option<usize> {
        let block_base = ptr & constants::BLOCK_PTR_MASK;

        if let Some(ref block) = self.all_blocks.get(&block_base) {
            let block_offset = ptr & !constants::BLOCK_PTR_MASK;
            let meta = unsafe { BlockMeta::attach(block.as_ptr()) };
            if block_offset < constants::ALLOC_UPPER_EXTENT && meta.is_object_marked(block_offset) {
                return Some(ptr);
            }
        }
        None
    }
}

/// A type that implements `AllocRaw` to provide a low-level heap interface.
/// Does not allocate internally on initialization.
// ANCHOR: DefStickyImmixHeap
pub struct ImmixConsHeap<H> {
    blocks: UnsafeCell<BlockList>,
    stack: SystemStackInfo,
    _header_type: PhantomData<*const H>,
}
// ANCHOR_END: DefStickyImmixHeap

impl<H: AllocHeader> ImmixConsHeap<H> {
    pub fn new() -> ImmixConsHeap<H> {
        ImmixConsHeap {
            blocks: UnsafeCell::new(BlockList::new()),
            stack: SystemStackInfo::new(),
            _header_type: PhantomData,
        }
    }

    /// Find a space for a small, medium or large object
    // TODO this just allocates a new block, but should look at
    // recycled blocks first
    fn find_space(
        &self,
        alloc_size: usize,
        size_class: SizeClass,
    ) -> Result<AllocDest, AllocError> {
        let blocks = unsafe { &mut *self.blocks.get() };
        blocks.find_space(alloc_size, size_class)
    }

    /// This function takes care of marking block attributes
    ///
    /// Safety:
    /// Assumes that the provided pointer is a valid object in a valid block
    ///
    /// Returns true if the object was already marked; otherwise false if it
    /// was never seen in the current mark iteration.
    unsafe fn mark(ptr: usize) -> bool {
        // 1. mark the object
        let header_ptr = Self::get_header(RawPtr::new(ptr as *mut ()));
        unsafe {
            let header = header_ptr.as_ref();
            if header.mark_is(Mark::Marked) {
                return true;
            }
            header.mark(Mark::Marked);
        }

        let block_base = (ptr & constants::BLOCK_PTR_MASK) as *const u8;
        let mut block_meta = unsafe { BlockMeta::attach(block_base) };

        // 2. mark the line
        let ptr_offset = ptr & !constants::BLOCK_PTR_MASK;
        block_meta.mark_line(ptr_offset / constants::LINE_SIZE);

        // 3. mark the block
        block_meta.mark_block();

        false
    }
}

impl<H: AllocHeader> AllocRaw for ImmixConsHeap<H> {
    type Header = H;

    /// Allocate space for object `T`, creating an header for it and writing the object
    /// and the header into the space
    // ANCHOR: DefAlloc
    fn alloc<T>(&self, object: T) -> Result<RawPtr<T>, AllocError>
    where
        T: AllocObject<<Self::Header as AllocHeader>::TypeId>,
    {
        // calculate the total size of the object and it's header
        let header_size = Self::Header::header_size();
        let object_size = size_of::<T>();
        let total_size = header_size + object_size;

        // round the size to the next word boundary to keep objects aligned and get the size class
        let size_class = SizeClass::get_for_size(total_size)?;

        // attempt to allocate enough space for the header and the object
        let dest = self.find_space(total_size, size_class)?;

        // instantiate an object header for type T, setting the mark bit to "allocated"
        let header = Self::Header::new::<T>(object_size as ArraySize, size_class, Mark::Allocated);

        // write the header into the front of the allocated space
        unsafe {
            write(dest.space as *mut Self::Header, header);
        }

        // write the object into the allocated space after the header
        let object_space = unsafe { dest.space.add(header_size) };
        unsafe {
            write(object_space as *mut T, object);
        }

        // Mark this object in the block's object map
        let object_offset = object_space.addr() - dest.block.addr();
        unsafe {
            let mut meta = BlockMeta::attach(dest.block);
            meta.mark_object(object_offset);
        }

        // return a pointer to the object in the allocated space
        Ok(RawPtr::new(object_space as *const T))
    }
    // ANCHOR_END: DefAlloc

    /// Allocate space for an array, creating an header for it, writing the header into the space
    /// and returning a pointer to the array space
    // ANCHOR: DefAllocArray
    fn alloc_array(&self, size_bytes: ArraySize) -> Result<RawPtr<u8>, AllocError> {
        // calculate the total size of the array and its header
        let header_size = Self::Header::header_size();
        let total_size = header_size + size_bytes as usize;

        // round the size to the next word boundary to keep objects aligned and get the size class
        let size_class = SizeClass::get_for_size(total_size)?;

        // attempt to allocate enough space for the header and the array
        let dest = self.find_space(total_size, size_class)?;

        // instantiate an object header for an array, setting the mark bit to "allocated"
        let header = Self::Header::new_array(size_bytes, size_class, Mark::Allocated);

        // write the header into the front of the allocated space
        unsafe {
            write(dest.space as *mut Self::Header, header);
        }

        // calculate where the array will begin after the padded header
        let array_space = unsafe { dest.space.add(header_size) };

        // Initialize array to zero
        let array = unsafe { from_raw_parts_mut(array_space as *mut u8, size_bytes as usize) };
        for byte in array {
            *byte = 0;
        }

        // Mark this object in the block's object map
        let object_offset = array_space.addr() - dest.block.addr();
        unsafe {
            let mut meta = BlockMeta::attach(dest.block);
            meta.mark_object(object_offset);
        }

        Ok(RawPtr::new(array_space))
    }
    // ANCHOR_END: DefAllocArray

    /// Return the object header for a given object pointer
    // ANCHOR: DefGetHeader
    fn get_header(object: RawPtr<()>) -> RawPtr<Self::Header> {
        let padded_header = Self::Header::header_size();
        unsafe {
            let header_ptr = (object.as_ptr() as *const u8).sub(padded_header) as *mut Self::Header;
            RawPtr::new(header_ptr)
        }
    }
    // ANCHOR_END: DefGetHeader

    /// Return the object from it's header address
    // ANCHOR: DefGetObject
    fn get_object(header: RawPtr<Self::Header>) -> RawPtr<()> {
        let padded_header = Self::Header::header_size();
        unsafe {
            let obj_ptr = (header.as_ptr() as *const u8).add(padded_header) as *mut ();
            RawPtr::new(obj_ptr)
        }
    }
    // ANCHOR_END: DefGetObject

    /// Run a garbage collection iteration
    fn gc<V: TraceVisitor>(&self, tracer: &mut V) -> Result<(), AllocError> {
        let blocks = unsafe { &mut *self.blocks.get() };

        // TODO
        // - reset line and block mark bits

        // 1. stack scan for things that could be pointers into the heap
        let mut stack_scan = Vec::new();
        self.stack.scan(&mut stack_scan, |ptr| {
            blocks.is_conservatively_a_ptr(ptr & Self::Header::TAG_MASK)
        });

        // 2. trace

        // 2.1 trace the stack scan
        for ptr in stack_scan.iter() {
            if !unsafe { Self::mark(*ptr) } {
                tracer.visit(RawPtr::new(*ptr as *const ()));
            }
        }

        // 2.2 trace the heap scan
        while let Some(object) = tracer.pop() {
            if !unsafe { Self::mark(object.addr()) } {
                tracer.visit(object);
            }
        }

        // 3. recycle, drop, allocate blocks
        blocks.manage_blocks()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::allocator::{AllocObject, AllocRaw, AllocTypeId, Mark, SizeClass, TraceVisitor};
    use std::cell::Cell;
    use std::slice::from_raw_parts;

    struct TestHeader {
        _size_class: SizeClass,
        mark: Cell<Mark>,
        type_id: TestTypeId,
        _size_bytes: u32,
    }

    #[derive(PartialEq, Copy, Clone)]
    enum TestTypeId {
        Array,
        Biggish,
        List,
        Stringish,
        Usizeish,
    }

    impl AllocTypeId for TestTypeId {}

    impl AllocHeader for TestHeader {
        type TypeId = TestTypeId;

        fn new<O: AllocObject<Self::TypeId>>(size: u32, size_class: SizeClass, mark: Mark) -> Self {
            TestHeader {
                _size_class: size_class,
                mark: Cell::new(mark),
                type_id: O::TYPE_ID,
                _size_bytes: size,
            }
        }

        fn new_array(size: u32, size_class: SizeClass, mark: Mark) -> Self {
            TestHeader {
                _size_class: size_class,
                mark: Cell::new(mark),
                type_id: TestTypeId::Array,
                _size_bytes: size,
            }
        }

        fn mark(&self, value: Mark) {
            self.mark.set(value)
        }

        fn mark_is(&self, value: Mark) -> bool {
            self.mark.get() == value
        }

        fn size_class(&self) -> SizeClass {
            SizeClass::Small
        }

        fn size(&self) -> u32 {
            8
        }

        fn type_id(&self) -> TestTypeId {
            self.type_id
        }
    }

    struct Big {
        _huge: [u8; constants::BLOCK_SIZE + 1],
    }

    impl Big {
        fn make() -> Big {
            Big {
                _huge: [0u8; constants::BLOCK_SIZE + 1],
            }
        }
    }

    struct List {
        next: Option<RawPtr<List>>,
        value: u8,
    }

    impl List {
        fn new(value: u8, next: RawPtr<List>) -> List {
            List {
                next: Some(next),
                value,
            }
        }

        fn tail(value: u8) -> List {
            List { next: None, value }
        }

        fn trace<V: TraceVisitor>(&self, v: &mut V) {
            if let Some(next) = self.next {
                v.visit(next.as_untyped());
            }
        }
    }

    impl AllocObject<TestTypeId> for List {
        const TYPE_ID: TestTypeId = TestTypeId::List;
    }

    impl AllocObject<TestTypeId> for Big {
        const TYPE_ID: TestTypeId = TestTypeId::Biggish;
    }

    impl AllocObject<TestTypeId> for String {
        const TYPE_ID: TestTypeId = TestTypeId::Stringish;
    }

    impl AllocObject<TestTypeId> for usize {
        const TYPE_ID: TestTypeId = TestTypeId::Usizeish;
    }

    struct TraceProxy {
        tracer: HeapTracer,
    }

    impl TraceProxy {
        fn new() -> TraceProxy {
            TraceProxy {
                tracer: HeapTracer::new(),
            }
        }
    }

    impl TraceVisitor for TraceProxy {
        fn visit(&mut self, object: RawPtr<()>) {
            let header = ImmixConsHeap::<TestHeader>::get_header(object);
            let header = unsafe { header.as_ref() };

            // Every type that is traced must be implemented here
            match header.type_id {
                TestTypeId::Array => unimplemented!(),
                TestTypeId::Biggish => unimplemented!(),
                TestTypeId::List => {
                    let list = object.cast::<List>();
                    if let Some(next) = unsafe { list.as_ref().next } {
                        self.tracer.visit(next.as_untyped());
                    }
                }
                TestTypeId::Stringish => unimplemented!(),
                TestTypeId::Usizeish => unimplemented!(),
            }
            self.tracer.visit(object);
        }

        fn pop(&mut self) -> Option<RawPtr<()>> {
            self.tracer.pop()
        }
    }

    #[test]
    fn test_memory() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        match mem.alloc(String::from("foo")) {
            Ok(s) => {
                let orig = unsafe { s.as_ref() };
                assert!(*orig == String::from("foo"));
            }

            Err(_) => panic!("Allocation failed"),
        }
    }

    #[test]
    fn test_too_big() {
        let mem = ImmixConsHeap::<TestHeader>::new();
        assert!(mem.alloc(Big::make()) == Err(AllocError::BadRequest));
    }

    #[test]
    fn test_many_obs() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        let mut obs = Vec::new();

        // allocate a sequence of numbers
        for i in 0..(constants::BLOCK_SIZE * 3) {
            match mem.alloc(i) {
                Err(_) => panic!("Allocation failed unexpectedly"),
                Ok(ptr) => obs.push(ptr),
            }
        }

        // check that all values of allocated words match the original
        // numbers written, that no heap corruption occurred
        for (i, ob) in obs.iter().enumerate() {
            assert!(i == unsafe { *ob.as_ref() })
        }
    }

    #[test]
    fn test_array() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        let size = 2048;

        match mem.alloc_array(size) {
            Err(_) => panic!("Array allocation failed unexpectedly"),

            Ok(ptr) => {
                // Validate that array is zero initialized all the way through
                let ptr = ptr.as_ptr();

                let array = unsafe { from_raw_parts(ptr, size as usize) };

                for byte in array {
                    assert!(*byte == 0);
                }
            }
        }
    }

    #[test]
    fn test_alignment() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        match mem.alloc(String::from("foo")) {
            Ok(s) => {
                let untyped_ptr = s.as_untyped();
                let header_ptr = ImmixConsHeap::<TestHeader>::get_header(untyped_ptr);
                let header_addr = header_ptr.as_ptr() as usize;
                let obj_addr = untyped_ptr.as_ptr() as usize;

                assert!(header_addr & (constants::ALLOC_ALIGN_BYTES - 1) == 0);
                assert!(obj_addr & (constants::ALLOC_ALIGN_BYTES - 1) == 0);

                let obj_from_header = ImmixConsHeap::<TestHeader>::get_object(header_ptr);
                assert!(obj_from_header.as_ptr() as usize == obj_addr);
            }

            Err(_) => panic!("Allocation failed"),
        }
    }

    #[test]
    fn test_object_map_marked_on_alloc() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        match mem.alloc(42usize) {
            Ok(ptr) => {
                let untyped_ptr = ptr.as_untyped();
                let block_base = (untyped_ptr.as_ptr() as usize) & constants::BLOCK_PTR_MASK;
                let object_offset = untyped_ptr.as_ptr() as usize - block_base;
                let meta = unsafe { BlockMeta::attach(block_base as *const u8) };
                assert!(meta.is_object_marked(object_offset));
            }

            Err(_) => panic!("Allocation failed"),
        }
    }

    #[test]
    fn test_header() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        match mem.alloc(String::from("foo")) {
            Ok(s) => {
                let untyped_ptr = s.as_untyped();
                let header_ptr = ImmixConsHeap::<TestHeader>::get_header(untyped_ptr);
                let header = unsafe { &*header_ptr.as_ptr() as &TestHeader };

                assert!(header.type_id() == TestTypeId::Stringish);
            }

            Err(_) => panic!("Allocation failed"),
        }
    }

    #[test]
    fn test_gc_stackscan() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        // keep a set of pointers on the stack
        const COUNT: usize = 100;
        let mut obs: [_; COUNT] = [mem.alloc(99).unwrap(); COUNT];
        for ptr in obs.each_mut() {
            *ptr = mem.alloc(99).unwrap();
        }

        // check that they're not marked yet
        for ptr in obs {
            let header: RawPtr<TestHeader> = ImmixConsHeap::get_header(ptr.as_untyped());
            assert!(unsafe { header.as_ref().mark_is(Mark::Allocated) });
        }

        let mut tracer = HeapTracer::new();
        mem.gc(&mut tracer);

        // now they should be marked
        for ptr in obs {
            let header: RawPtr<TestHeader> = ImmixConsHeap::get_header(ptr.as_untyped());
            assert!(unsafe { header.as_ref().mark_is(Mark::Marked) });
        }
    }

    #[test]
    fn test_gc_trace() {
        let mem = ImmixConsHeap::<TestHeader>::new();

        const COUNT: u8 = 100;
        let mut head = mem.alloc(List::tail(0)).unwrap();
        for i in 1..COUNT {
            head = mem.alloc(List::new(i, head)).unwrap();
        }

        let mut tracer = TraceProxy::new();
        mem.gc(&mut tracer);

        // such unsafe!
        unsafe {
            let mut count = 99;
            while let Some(next) = head.as_ref().next {
                let header: RawPtr<TestHeader> = ImmixConsHeap::get_header(head.as_untyped());
                assert!(header.as_ref().mark_is(Mark::Marked));
                assert!(head.as_ref().value == count);
                head = next;
                count -= 1;
            }
            assert!(count == 0);
        }
    }
}
