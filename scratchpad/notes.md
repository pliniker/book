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
- [ ] block management
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

