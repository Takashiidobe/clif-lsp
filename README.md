# clif-lsp

A language server for
[Cranelift IR](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/docs/ir.md)
(CLIF) text files. It gives your editor diagnostics, navigation, completion,
formatting, and views of the CFG and compiled output for `.clif` files.

## Install

You need a Rust toolchain.

```sh
cargo install clif-lsp # or cargo binstall
```

To build from a checkout instead, run `cargo install --path .`.

This puts a `clif-lsp` binary on your `PATH`. It speaks LSP over stdio and takes
no arguments.

Or download a prebuilt binary for Linux, macOS, or Windows from the
[releases page](https://github.com/Takashiidobe/clif-lsp/releases) and put it on
your `$PATH`.

For Neovim setup, see [docs/neovim.md](docs/neovim.md). Any editor that can
launch a stdio language server works the same way.

## What you get

- **Diagnostics** from the tolerant parser, name resolution, `cranelift-reader`,
  and the Cranelift verifier.
- **Navigation**: go to definition, references, rename, document highlight,
  document symbols, call hierarchy.
- **Hover** with opcode docs, inferred value types, dominators, and liveness.
- **Completion** for opcodes, types, condition codes, flags, and names in scope.
- **Signature help** for opcodes and calls.
- **Inlay hints** for inferred result types and branch argument names.
- **Code actions**: fix unknown opcodes and undefined names, add missing
  terminators, add block parameters, remove unused declarations, collapse
  `brif`, reuse duplicate constants, split blocks.
- **Formatting** of whole documents and ranges.
- **Folding, selection ranges, and semantic tokens.**
- **Custom requests** for the control flow graph, dominance and liveness, and
  the optimized or compiled form of a function.

Every feature is described in [docs/features.md](docs/features.md). The custom
requests are in [docs/requests.md](docs/requests.md).

## Features

### Control flow graph

Draws each block with its instructions and the edges between blocks.

![Control flow graph](docs/images/cfg.gif)

### Optimization passes

Steps through each optimizer pass and shows what it added and removed.

![Optimization passes](docs/images/passes.gif)

### E-graph

Shows the equivalent forms the optimizer considered for each value.

![E-graph](docs/images/egraph.gif)

### Provenance

Maps each optimized instruction back to the input instructions it came from.

![Provenance](docs/images/provenance.gif)

### Register allocation

Shows where the register allocator put each value, with spills, reloads and
pressure.

![Register allocation](docs/images/regalloc.gif)

### Live ranges

Shows where each value is live, instruction by instruction.

![Live ranges](docs/images/live_ranges.gif)

### Disassembly

Shows the machine code for a function, grouped under the CLIF instruction that
produced it.

![Disassembly](docs/images/disasm.gif)

### Definitions and uses

Shows where a value is defined and the tree of instructions that use it.

![Definitions and uses](docs/images/def_use.gif)

### Use counts

Lists the values that are never used and the values used exactly once.

![Use counts](docs/images/use_counts.gif)

## Limits

- The server works on one open file at a time. It does not read other files in
  the workspace.
- Semantic diagnostics and the stage views use the Cranelift version the server
  is built against. Text that a different version accepts may be rejected.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at
your option.
