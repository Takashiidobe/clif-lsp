# clif-lsp

A language server for
[Cranelift IR](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/docs/ir.md)
(CLIF) text files. It gives your editor diagnostics, navigation, completion,
formatting, and views of the CFG and compiled output for `.clif` files.

## Install

You need a Rust toolchain.

```sh
cargo install clif-lsp
```

To build from a checkout instead, run `cargo install --path .`.

This puts a `clif-lsp` binary on your `PATH`. It speaks LSP over stdio and takes
no arguments.

To use it in Neovim, see [docs/neovim.md](docs/neovim.md). Any editor that can
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
  terminators, remove unused declarations.
- **Formatting** of whole documents and ranges.
- **Folding, selection ranges, and semantic tokens.**
- **Custom requests** for the control flow graph, dominance and liveness, and
  the optimized or compiled form of a function.

Every feature is described in [docs/features.md](docs/features.md). The custom
requests are in [docs/requests.md](docs/requests.md).

## Limits

- The server works on one open file at a time. It does not read other files in
  the workspace.
- Semantic diagnostics and the stage views use the Cranelift version the server
  is built against. Text that a different version accepts may be rejected.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at
your option.

## Contributing

Tests are `.clif` fixtures run with `cargo test --test fixtures`. See
[tests/fixtures/README.md](tests/fixtures/README.md) for the directives.
