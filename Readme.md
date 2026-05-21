# sea — A Static Memory Safety Analyzer for C

`sea` is a static analysis tool that detects memory safety violations in C code, inspired by Rust's borrow checker. It parses C source files, builds a Control Flow Graph (CFG), and performs ownership analysis to catch bugs before they happen at runtime.

## Features

### Error Detection
- **Double free** — freeing a pointer that has already been freed
- **Use after free** — dereferencing a pointer after it has been freed
- **Null pointer dereference** — dereferencing a pointer assigned `NULL`
- **Uninitialized pointer use** — using a pointer before it has been assigned
- **Memory leak** — heap-allocated memory that goes out of scope without being freed
- **Stack variable address return** — returning the address of a local variable
- **Pointer outlive scope** — a pointer that outlives the variable it points to
- **Pass freed pointer** — passing a freed pointer to a function
- **Field access on freed pointer** — accessing a struct field through a freed pointer

### Warnings (Conditional Paths)
When a violation only occurs on some paths through the code, `sea` reports a warning instead of an error:
- Possible double free
- Possible use after free

### Control Flow Support
- Linear code
- `if` / `else` (including nested)
- `while` loops
- `for` loops
- `do-while` loops
- `switch` statements with fallthrough detection

## Installation

Requires Rust and Cargo.

```bash
git clone <repo>
cd sea
cargo build --release
```

## Usage

```bash
cargo run -- path/to/file.c
```

Or after building:

```bash
./target/release/sea path/to/file.c
```

### Example Output

```
examples/double_free.c:6:7: error: double free of 'p'
examples/null_dereference.c:4:3: error: null pointer dereference of 'p'
examples/conditional_free.c:7:3: warning: possible use after free of 'p'
examples/leak.c:9:1: error: memory leak of pointer 'p'
```

## How It Works

`sea` uses a three-stage pipeline:

```
C source
   ↓  tree-sitter-c        (parsing)
  CST
   ↓  cfg.rs               (CFG construction)
  CFG (petgraph DiGraph)
   ↓  sea.rs               (ownership analysis)
  Diagnostics
```

**1. Parsing** — tree-sitter parses the C source into a Concrete Syntax Tree (CST).

**2. CFG Construction** (`cfg.rs`) — the CST is walked and lowered into a Control Flow Graph. Branching constructs (`if`, `while`, `for`, `switch`) create multiple basic blocks connected by edges. Each basic block contains a list of statements relevant to memory safety (`Malloc`, `Free`, `Deref`, etc.).

**3. Ownership Analysis** (`sea.rs`) — a worklist algorithm processes blocks in dependency order. Each block gets an incoming `BlockState` (a map of variable names to ownership states) computed by merging the outgoing states of all predecessor blocks. For loops, blocks are reprocessed until the state stabilizes (fixed point iteration).

### Ownership States

| State | Meaning |
|---|---|
| `Allocated` | Heap allocated, valid to use |
| `Freed` | Has been freed, unsafe to use |
| `MaybeFreed` | Freed on some paths — warning |
| `Uninitialized` | Declared but not assigned |
| `Null` | Assigned NULL |
| `OutOfScope` | Variable has gone out of scope |
| `Returned` | Ownership transferred to caller |

### MaybeFreed

When paths merge (e.g. after an `if/else`), ownership states are combined. If a variable is `Freed` on one path and `Allocated` on another, it becomes `MaybeFreed` — triggering a warning instead of a hard error, since the violation only occurs conditionally.

## Architecture

```
src/
  main.rs          — CLI entry point (clap)
  sea.rs           — ownership analysis, diagnostic handlers
  cfg.rs           — CFG construction, basic blocks, block state
  analyzer_state.rs — OwnershipState enum
  variable_info.rs  — VariableInfo struct (state, scope, alloc kind)
  diagnostics.rs    — Diagnostic struct and display
  tests.rs          — integration tests
examples/
  *.c              — example C files used in tests
```

## Running Tests

```bash
cargo test
```

All 31 tests cover linear code, branching, loops, scope tracking, and all error categories.

## Limitations

- **Single-file analysis** — all functions in a file are analyzed together in one CFG; interprocedural analysis is not yet supported
- **No type inference** — pointer detection is based on usage patterns (malloc/free/deref) rather than type information
- **No preprocessor** — `#define` macros and conditional compilation are not evaluated
