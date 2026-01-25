# Project Overview

## Goals

An educational tool for building and understanding programming language
implementaion supporting the following features:

- Conservative Immix garbage collection
- Bytecode compiler and virtual machine interpreter
- Dynamic duck-typed semantics
- Less emphasis on syntax, therefore an s-expression syntax

Given the educational emphasis, simplicity should be maintained.
- No heavy optimizations
- Readability of code is paramount
- Maintainability of code is paramount

## Layout

There are three Rust projects within this repository:

- `interpreter` - the lexer, parser, compiler, interpreter and language semantics
- `immixcons` - an implementation of a Conservative Immix garbage collector
- `blockalloc` - a fixed size and alignment memory block allocator

There is also documentation under `booksrc`, a markdown book describing design
choices and implementation.

## Building

The main application resides in `interpreter` and can be built using standard
Rust tooling:

- debug: `cargo build`
- release: `cargo build --release`

Individual components (`immixcons` and `blockalloc`) can also be simply built
using the standard Rust tooling.

The `booksrc` can be built from within that directory using `mdbook` which
must be installed.

## Testing

Each project contains unit tests that can be run by:

```
cargo test
```

## Code style

Standard rustfmt rules apply universally.

## Planning

Plans and notes should be kept under the `scratchpad/` directory.
Refer to `scratchpad/notes.md` for high level plans.
