# Register allocation visibility (public API only)

Spike for `clif-lsp-bfu.3`. Question: what register-allocation data can we show
using stock `cranelift-codegen =0.136.2`, with no patched or vendored copy?
Everything below was run against 0.136.2 on x86_64 (regalloc2 0.15.2).

## What is reachable

| Source | How | What it holds |
|---|---|---|
| Post-regalloc vcode text | `Context::set_disasm(true)`, then `compiled_code().vcode` | One line per machine instruction with physical registers, spill/reload `<offset:N>+(%rsp)` operands, and the prologue/epilogue. Moves inserted by the allocator are ordinary `mov` lines with no marker. |
| Value locations | `func.collect_debug_info()`, a `ValueLabelStart` per value, then `compiled_code().value_labels_ranges` | For each labelled CLIF value, a list of `{loc, start, end}` over machine-code offsets. `loc` is `Reg(p7i)` or `CFAOffset(-80)`. A value that is spilled shows up as alternating `Reg` and `CFAOffset` ranges. |
| Instruction to code offsets | `func.set_srcloc(inst, n)` per instruction, then `buffer.get_srclocs_sorted()` | `{start, end, loc}` code ranges, so a CLIF instruction maps to a byte range. |
| Block layout | `machine_code_cfg_info` setting, `bb_starts`, `bb_edges` | Code offset of each block. |
| `regalloc2::Output` | Add `regalloc2 = "=0.15.2"` as a direct dependency, call `Context::optimize` yourself, then `isa.compile_function(.., &mut regalloc2::Ctx::default(), ..)` and read `ctx.output` | `allocs` (per-operand allocation), `inst_alloc_offsets`, `edits` (every `Move` between registers and `stackN` slots, keyed by program point), `num_spillslots`, and `stats`. |
| regalloc2 trace log | Enable the stock `trace-log` feature of `cranelift-codegen`, install a `log::Log` | Free-text dump of live ranges, bundles, uses with spill weights, and eviction/split decisions, about 700 lines for a 4-instruction function and 3500 for a 33-instruction one. |

Notes on the `regalloc2::Output` route:

- `Ctx` has private fields, but `output` is public and is not cleared after the
  call. The pinned `=0.136.2` keeps the type identical to the one in
  `cranelift-codegen`.
- The program points in `edits` and the indexes in `inst_alloc_offsets` are
  vcode instruction indexes. The vcode itself is consumed during emit and is not
  returned, so there is no public mapping from a vcode index to a CLIF
  instruction or to a line of the disassembly text. Edits and instructions can
  be aligned with the text by counting lines per block, but a lowered
  instruction that expands to several machine instructions breaks the count.
- `stats` is filled by the default backtracking allocator
  (`splits`, `evict_bundle_count`, `spill_bundle_count`, `edits_count`) and is
  all zeros for `regalloc_algorithm = "single_pass"`. `edits` and `allocs` are
  filled by both.
- `ctx.output` does not contain live ranges, bundles, or interference. Those
  structures live in the private fields of `Ctx` and in `ion::Env`, which is
  not exposed.

## Decision per feature

| Feature | Verdict | Source | Limits |
|---|---|---|---|
| Virtual to physical assignments | Go | `value_labels_ranges` | Per CLIF value and per code-offset range, including where a value moves between a register and a stack slot. Values that optimization removes do not appear, since labels attach to the optimized function. Needs a label per value, which the CLIF text parser does not set, so we set them. |
| Spill and reload highlighting | Go, with a caveat | `value_labels_ranges` (`CFAOffset` ranges) and `edits` | A `CFAOffset` range says a value is in memory, and the transition points are the spill and reload. Pinning each to a CLIF line goes through `get_srclocs_sorted`, which is exact only to instruction granularity. The vcode text cannot tell a spill `mov` from any other `mov`. |
| Allocation moves | Go, text-level | `edits`, vcode text | The count and the from/to allocation of every move are exact. Showing them inline against the disassembly relies on the line-counting alignment above, so treat it as best effort. |
| Register pressure | Go, derived | `value_labels_ranges` | Count of simultaneously live registers per code offset, computed by us. It reflects only labelled values, so it undercounts temporaries the lowering creates. `num_spillslots` and `stats` give the totals. |
| Live ranges | Partial | `value_labels_ranges` | The assigned location range of each CLIF value, not regalloc2's live ranges. No bundles, no split points as regalloc2 sees them. |
| Interference | No-go | none | Not exposed by the output or any public type. The `trace-log` text mentions conflicts during processing, but it is unstructured, relies on a global logger and a non-default feature, and is not a stable format to parse. |
| Why was this spilled | No-go | none | Expected. The reasons (eviction by a higher spill weight, no free register in the range) exist only in the trace text and in `stats` counters, with no link from a counter to a specific value. |

## Costs and risks for the follow-on beads

- `compile_function` bypasses `Context::compile`, so we have to run
  `Context::optimize` and compute the CFG and dominator tree ourselves. This
  matches what `compile_stencil` does today, but it is a coupling to
  cranelift internals that a version bump can break. Pinning `=0.136.2` already
  gates that.
- `regalloc2` becomes a direct dependency, pinned to the version
  `cranelift-codegen` uses.
- Setting source locations and value labels changes the `Function` we compile,
  so it must be done on a clone, never on the function used for diagnostics.
- `trace-log` is a compile-time feature of `cranelift-codegen`. Turning it on
  slows compilation for every user and its text is not API, so we should not use
  it for anything shipped.

## Recommendation

- `clif-lsp-bfu.4` (assignments, spills and moves): proceed, built on
  `value_labels_ranges` plus `regalloc2::Output.edits`.
- `clif-lsp-bfu.5` (pressure, live ranges, interference): proceed only for
  pressure and per-value location ranges. Drop interference and "why was this
  spilled" as not possible without patching cranelift-codegen.
