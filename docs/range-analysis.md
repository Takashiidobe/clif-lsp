# Value range facilities in Cranelift (public API only)

Survey for `clif-lsp-abk.1`. Question: does `cranelift-codegen =0.136.2` expose
value ranges, known bits or similar facts that our range analysis can use or be
checked against? Checked by reading the 0.136.2 source and comparing with 0.125.4
and 0.130.0 from the local cargo registry.

**Decision: none.** Cranelift gives us nothing to build on or to compare with.
We write our own forward interval and known-bits analysis (`clif-lsp-abk.2`) and
check it in the fixtures against concrete execution instead (see
[Oracle](#oracle)).

## Proof-carrying code is gone

The facts we were looking for, `enable_pcc` and `ir::pcc::Fact` with its
`Range` variant, were Cranelift's proof-carrying code. They exist in 0.125.4 and
0.130.0 (the setting is in `cranelift-codegen-meta`, the checker is referenced
from `egraph`, `print_errors`, `settings`, `write` and `result`). In 0.136.0 and
0.136.2 the word `pcc` does not appear anywhere in `cranelift-codegen` or its
meta crate. The setting, the `Fact` type and the checker were removed between
0.130.0 and 0.136.0.

Even where it existed, PCC was a verifier for memory accesses in the backend:
it checked facts that a producer attached to values, it did not infer ranges for
arbitrary CLIF. It could not have served as an analysis.

## Nothing else is exposed

- Known bits and value ranges used by the optimizer live in ISLE rules under the
  private `opts` module. `egraph` is private too (see
  [egraph-visibility.md](egraph-visibility.md)). None of it is reachable.
- `src/spec/*.isle` are verification specs for the Crocus tool. They are inputs
  to a build-time check, not a runtime API.
- `ranges.rs` is a list of contiguous index ranges, unrelated to values.
- `souper_harvest` (behind the `souper-harvest` feature) turns a function into
  Souper IR text. It does not carry range or known-bits annotations.
- `ValueLabelsRanges` and `LabelValueLoc` are register and stack locations of
  values over code offsets. `regalloc` and `disasm` already use them.

## Oracle

Cranelift cannot be the oracle, but running the code can. The separate
`cranelift-interpreter` crate (versioned with Cranelift) evaluates CLIF on
concrete values. A fixture check can run a function on sample inputs, including
the edge values of each argument type, and assert that every result lies inside
the interval and agrees with the known bits our analysis reported. That checks
soundness (the analysis never excludes a reachable value) but not precision.

`cranelift-interpreter` is not in the local registry, so its API and its
coverage of the integer opcodes at `=0.136.2` are unchecked. Confirm both at the
start of `clif-lsp-abk.2` and record the result there. If it is not usable, the
fallback is a table of hand-computed expected ranges per opcode in the fixtures.
