# Notes

## TODOs

ImmixCons:

- [X] identifying valid root pointers from stack scan
  - [X] refactor: BumpBlock should only be used for head and overflow; should
        only have a Block::as_mut_ptr() ptr
  - [X] allocator needs to mark object map when object written
  - [X] root pointer id needs to check object map
- [X] stack scan logic
- [X] object tracing via object header
  - [X] trace within interpreter objects
  - [X] array trace
  - [X] dict trace
- [X] marking objects, lines, object maps, blocks
- [ ] recycle blocks
  - this means analyzing line occupancy and, based on a threshold, putting
  - blocks back into a "free" list
- [ ] pinning rooted objects
- [ ] evacuating blocks
- [ ] back pressure on allocation, triggering gc
- [ ] large objects

Interpreter improvements:

- [ ] replace Pairs with Arrays in parser and compiler
- [ ] additional types: integers, arbitrary sized integers
- [ ] implement some additional builtins - math operators, strings
- [ ] implement some integration tests

## Conservative Immix

- https://www.steveblackburn.org/pubs/papers/consrc-oopsla-2014.pdf
- https://docs.rs/portable-atomic/latest/portable_atomic/struct.AtomicUsize.html
- https://www.hboehm.info/gc/gcdescr.html

### Rooting

Conservative stack scanning.
- allows for intrusive data structures _where used_
- simpler mutator root management
  - interior mutability means roots only have to be readonly &ptr
  - which means no mem::replace etc if we have a phantom lifetime
- need to push all registers to stack
  - how is this safely done? bdwgc endorses use of getcontext() or setjmp
- need to find stack base
  - pthread_attr_getstack

Depends on:
- fast map of pointer to block
  - vec + heap?
- object map in each block
  - FIRST step, implement object block

### Tracing

Precise object scanning. OR could it be conservative?

This _just_ needs:
 - pointer values, not any hard data on types
 - to get object headers from pointers to set mark bits
 - read only to object memory space

---
# 🤖 Block management

## Immix Block Analysis for Reuse and Defragmentation

Immix (Blackburn & McKinley, 2008) is a mark-region collector that operates on a two-level heap structure: **blocks** (~32KB) subdivided into **lines** (~128 bytes). Block analysis exploits this hierarchy for both allocation efficiency and defragmentation decisions.

---

### Line Marking as the Basis for Analysis

During the mark phase, Immix marks at **line granularity**, not object granularity. A line is conservatively marked if *any* live object overlaps it. This produces a bitmap of marked/unmarked lines per block after collection.

This creates the primitive that all subsequent analysis is built on: **holes** — contiguous sequences of unmarked lines within a block. The shape and distribution of holes determines a block's fate.

---

### Block Classification

After collection, each block is categorized based on hole analysis:

| State | Criterion | Treatment |
|---|---|---|
| **Free** | No live lines | Returned to global free list immediately |
| **Recyclable** | Some holes, below occupancy threshold | Added to reuse candidate list |
| **Full** | Occupancy above threshold | Left in place; not worth touching |

The occupancy threshold is a tunable parameter. The original paper uses roughly a "sparseness" heuristic — if a block has enough free line space to be worth threading an allocator through it, it's recyclable.

---

### Reuse (Bump Allocation into Holes)

Recyclable blocks are reused by threading a **bump pointer through their holes**. The allocator:

1. Scans the line bitmap for the next hole
2. Sets a bump pointer to the start of the hole
3. Allocates sequentially until the hole is exhausted
4. Scans for the *next* hole, repeating

This avoids the overhead of a free-list per object. The key insight is that holes are typically large enough (multi-line) to amortize the scanning cost. Blocks with many small, scattered holes are deprioritized in favor of blocks with larger contiguous holes — there's an implicit sorting by hole quality.

---

### Opportunistic Defragmentation (Evacuation)

Immix adds an optional evacuation pass that is the more interesting case. It's **not run every collection** — it's triggered when fragmentation metrics cross a threshold. The mechanism:

1. **Identify evacuation candidates**: Blocks with very low occupancy (many holes, few live lines) — the "tail" of the occupancy distribution. These are cheaper to evacuate than recycle repeatedly.

2. **Select target blocks**: Free or near-free blocks are reserved as evacuation targets.

3. **Copy live objects out**: During the *next* mark phase, objects in candidate blocks are forwarded rather than just marked. This is piggybacked onto marking to avoid an extra heap traversal.

4. **Install forwarding pointers**: Evacuated objects leave a forwarding pointer; references are updated lazily or eagerly depending on implementation.

5. **Release evacuated blocks**: Once all live objects are forwarded, the source block becomes free.

The critical design decision is that evacuation is **opportunistic** — it only happens when free blocks are available to serve as targets. If the heap is under pressure, Immix degrades gracefully to pure mark-in-place with reuse, avoiding evacuation overhead.

---

### What Drives the Defragmentation Decision

The trigger for evacuation is typically a combination of:

- **Free block availability**: If fewer than *N* free blocks remain after collection, evacuation is too risky (you'd be evacuating *into* fragmented space)
- **Fragmentation rate**: If the ratio of recyclable-to-free blocks exceeds a threshold, repeated reuse of fragmented blocks is causing allocation overhead
- **Hole size distribution**: Many small holes in recyclable blocks signal that bump-pointer reuse is becoming inefficient

---

### One Nuance Worth Noting

The line-level marking means Immix has a **conservative element** at the line boundary: if a live object *starts* in a line but a dead object also occupies that line, the entire line is marked live. This can cause **floating garbage** — dead objects kept alive until the block is eventually evacuated or the objects happen to be the sole occupant of a line. In practice this is bounded (one line = 128 bytes of potential float per dead object), but it means reuse analysis slightly overestimates live data, and defragmentation decisions based purely on line bitmaps are not perfectly accurate.

---

**Summary**: Block reuse in Immix is driven by hole structure derived from line bitmaps; defragmentation is an opportunistic evacuation pass gated on free-block availability and fragmentation pressure, piggybacked onto marking to minimize overhead.

---

## The Occupancy Threshold in Detail

This is worth unpacking carefully because there are actually **two related but distinct heuristics** bundled under "sparseness," and the original paper is somewhat qualitative here — specific threshold values are largely implementation choices rather than theoretically derived constants.

---

### What "Occupancy" Actually Measures

Occupancy in Immix is measured in **lines**, not bytes. A block's occupancy is:

```
occupied_lines / total_lines_per_block
```

Because of conservative line marking, this is an **overestimate of live data** — a line is counted occupied if any live byte touches it. So a block at 60% line occupancy might contain substantially less than 60% live data by bytes. This matters for threshold calibration: if you're using line counts to decide whether to evacuate, you're operating on an upper bound of actual liveness.

---

### The Two Separate Thresholds

**Threshold 1: Recyclable vs. Full (allocation policy)**

This governs whether a block is worth returning to the allocator for bump-pointer reuse. The relevant question is: *is there enough free space in this block's holes to justify the overhead of scanning through them?*

The cost model here is:
- Each hole requires scanning the line bitmap to find its start/end
- Each hole boundary requires resetting the bump pointer
- Small, numerous holes have high overhead per byte reclaimed

So the threshold isn't purely about total free space — it's about **hole structure**. A block with 40% free lines concentrated in 2-3 large holes is substantially more valuable than a block with 40% free lines scattered across 20 single-line holes. The original Immix paper acknowledges this but doesn't fully formalize it into a single scalar threshold; implementations typically use a minimum free-line count as a proxy.

**Threshold 2: Evacuation candidate selection (defragmentation policy)**

This is a *lower* occupancy bound — blocks that are *too sparse* become evacuation candidates. The intuition: a block at 10% occupancy is wasting 90% of its space to retain a handful of live objects. Evacuating them and freeing the block is better than repeatedly threading the allocator through its holes.

The evacuation threshold is typically set quite low — you only want to evacuate blocks where the copying cost is small relative to the space reclaimed. Evacuating a block at 50% occupancy is expensive (copy half the block's worth of data) and only frees one block. Evacuating a block at 5% occupancy is cheap and frees almost an entire block.

---

### The Gap Between the Two Thresholds

This is the part that's easy to miss. There's a **middle zone** of blocks that are:
- Too sparse to be "full" (below the recycle threshold)
- Not sparse enough to be worth evacuating (above the evacuation threshold)

These blocks get recycled repeatedly — the allocator threads through their holes each cycle — without ever being compacted. This is intentional: evacuation has real costs (copying, forwarding pointer installation, reference updating), and blocks in this middle zone don't have a poor enough space/live-data ratio to justify those costs.

Over time, if fragmentation accumulates in this middle zone, the pressure on the evacuation threshold increases — more blocks fall below it — which is one of the signals that triggers a defragmentation cycle.

---

### What the Paper Leaves Underspecified

The Blackburn & McKinley paper evaluates Immix empirically and reports that it performs well across a range of threshold choices, which suggests the heuristic is **not highly sensitive** to exact values within a reasonable range. The practical implication is:

- The thresholds are tunable parameters, not analytically derived constants
- They interact with heap size (a larger heap can tolerate more fragmentation before evacuation pressure builds)
- They interact with allocation rate (a high-allocation workload exhausts recyclable blocks faster, shifting the balance toward evacuation)

This is worth flagging as an epistemic caveat: claims you might encounter about specific threshold values (e.g., "Immix uses 25% as the recyclable threshold") should be treated as implementation-specific rather than definitional to the algorithm.

---

### A Falsification-Oriented Note

If you're implementing this and trying to calibrate thresholds empirically, the most informative measurement isn't just GC pause time — it's **the ratio of holes-scanned to bytes-allocated** during recycling passes. If that ratio is climbing, your recycle threshold is too permissive (you're accepting blocks with poor hole structure). If evacuation cycles are frequent and expensive, your evacuation threshold is too high (you're evacuating blocks that weren't sparse enough to justify the copy cost).
