# Value range facilities in Cranelift (public API only)

Survey for `clif-lsp-abk.1`. Question: does `cranelift-codegen =0.136.2` expose
value ranges, known bits or similar facts that a range analysis can use or be
checked against? Checked by reading the 0.136.2 source, comparing it with 0.125.4,
0.130.0 and 0.137.0-rc.1 from the local cargo registry, and reading the upstream
issues and pull requests linked below.

**Decision: blocked.** Cranelift exposes no value range or known-bits facility,
and nothing is planned upstream. The project rule is to lean on Cranelift rather
than write an analysis of our own that could disagree with it, so the range
analysis epic (`clif-lsp-abk`) waits. Nothing is built.

## What was there: proof-carrying code

`enable_pcc` and `ir::pcc::Fact` (with a `Range` variant) are Cranelift's
proof-carrying code. They exist in 0.125.4 and 0.130.0 and are gone in 0.136.0,
0.136.2 and 0.137.0-rc.1.

Even when present it was not a range analysis. The module's own documentation
says facts are supplied by the producer of the IR, that Cranelift does not
derive and forward-propagate them eagerly, and that this avoids building "a
knowledge base that effectively encodes the integer ranges of many or most
values". It checked memory-access sequences after lowering. A revived PCC would
not fill this gap either.

## Why it was removed

[wasmtime#12800](https://github.com/bytecodealliance/wasmtime/pull/12800),
"PCC: remove proof-carrying code (for now?)", merged 2026-03-31. Its description
gives the reasons:

- It was an experiment that worked for static Wasm memories, never for dynamic
  ones.
- Work on it lost priority before it reached fuzzing or CI.
- It bit-rotted: new backend optimizations and lowerings were added without
  updating the checker.
- The author still thinks it can be built and has ideas for how, but wants it
  rebuilt on a separate experiment branch, with robust behaviour across all
  memory configurations, before it merges.

The removal was prompted by a regression report,
[wasmtime#12497](https://github.com/bytecodealliance/wasmtime/issues/12497),
where PCC validation failed with `UnimplementedInst` after upgrading from
Wasmtime 37 to 38. The maintainer's reply there says PCC was not tested in CI,
so more such failures were expected, and that its documentation was
"aspirational".

## Where it could come back

Nothing is scheduled. Places to watch:

- A PCC revival, per the plan in #12800. It would appear as `ir::pcc` and an
  `enable_pcc` setting in a new `cranelift-codegen` release. Only
  producer-supplied facts, as above.
- [wasmtime#10150](https://github.com/bytecodealliance/wasmtime/issues/10150),
  a proposed single CLIF operator for dynamic Wasm bounds checks. It argues that
  verifying the small pieces of a bounds check is brittle and that a verifier
  wants one operation with a verified implementation. If it lands, a revived PCC
  may be built on it.
- [wasmtime#7965](https://github.com/bytecodealliance/wasmtime/issues/7965), the
  earlier design discussion on making range facts more general. It shows what
  the fact language could express.
- A search of the upstream issues for known-bits or integer-range analysis in
  the mid-end found nothing open. The optimizer's known-bits logic lives in
  private ISLE rules (`opts`, `egraph`), which are not reachable.

When bumping the pinned Cranelift version, check for `enable_pcc`, `ir::pcc`
and any new analysis module in `cranelift-codegen/src/lib.rs` before
unblocking the epic.

## Nothing else is exposed

- `src/spec/*.isle` are verification specs for the Crocus tool, a build-time
  check and not a runtime API.
- `ranges.rs` is a list of contiguous index ranges, unrelated to values.
- `souper_harvest` (behind the `souper-harvest` feature) turns a function into
  Souper IR text. It carries no range or known-bits annotations.
- `ValueLabelsRanges` and `LabelValueLoc` are register and stack locations of
  values over code offsets. `regalloc` and `disasm` already use them.

## Not a way around it

- `cranelift-interpreter` evaluates CLIF on concrete values and could check
  soundness of an analysis, but it is not a source of ranges. It is also not
  a reliable oracle for every opcode:
  [wasmtime#14568](https://github.com/bytecodealliance/wasmtime/issues/14568)
  reports that it traps on `srem INT_MIN, -1` where Cranelift defines the result
  as 0. It is not in the local registry, so its API at `=0.136.2` is unchecked.
- The mid-end constant folder, driven by replacing inputs with `iconst`s, would
  give Cranelift's own result at single points. That checks opcode semantics, not
  intervals, and was not tried.
