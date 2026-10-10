# Range analysis

clif-lsp does not show value ranges or known bits, because Cranelift 0.136.2
exposes neither. The closest thing it had, proof-carrying code (`enable_pcc` and
`ir::pcc::Fact`), was removed in
[wasmtime#12800](https://github.com/bytecodealliance/wasmtime/pull/12800): it
only worked for static Wasm memories, was never fuzzed or run in CI, and
bit-rotted as backends changed. It was not a range analysis anyway, since the IR
producer supplied the facts and Cranelift did not derive them. The optimizer's
known-bits logic sits in private ISLE rules that a dependent crate cannot reach.
To add this feature we need Cranelift to publish a range or known-bits analysis,
for example a revived PCC (watch for `ir::pcc` and `enable_pcc` in new
`cranelift-codegen` releases, and for
[wasmtime#10150](https://github.com/bytecodealliance/wasmtime/issues/10150)), or
we write our own analysis and accept that it can disagree with the optimizer.
Check `cranelift-codegen/src/lib.rs` for either whenever the pinned version
moves.
