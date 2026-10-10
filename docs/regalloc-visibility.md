# Register allocation data from the public API

The `regalloc` view shows where the allocator put each value. This note covers
which data is available and what each view can claim. It was checked against
`cranelift-codegen =0.136.2` on x86_64 (regalloc2 0.15.2), with no patched or
vendored copy.

## Sources

| Source                     | How                                                                                                                                                              | What it holds                                                                                                                                                                                                            |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Post-regalloc vcode text   | `Context::set_disasm(true)`, then `compiled_code().vcode`                                                                                                        | One line per machine instruction with physical registers, spill and reload operands such as `<offset:N>+(%rsp)`, and the prologue and epilogue. Moves inserted by the allocator are ordinary `mov` lines with no marker. |
| Value locations            | `func.collect_debug_info()`, a `ValueLabelStart` per value, then `compiled_code().value_labels_ranges`                                                           | For each labelled CLIF value, a list of `{loc, start, end}` over code offsets. `loc` is `Reg(p7i)` or `CFAOffset(-80)`. A spilled value alternates between the two.                                                      |
| Instruction to code offset | `func.set_srcloc(inst, n)` per instruction, then `buffer.get_srclocs_sorted()`                                                                                   | `{start, end, loc}` code ranges, so a CLIF instruction maps to a byte range.                                                                                                                                             |
| Block layout               | `machine_code_cfg_info` setting, `bb_starts`, `bb_edges`                                                                                                         | The code offset of each block.                                                                                                                                                                                           |
| `regalloc2::Output`        | A direct `regalloc2 = "=0.15.2"` dependency, then `Context::optimize` and `isa.compile_function(.., &mut regalloc2::Ctx::default(), ..)`, then read `ctx.output` | `allocs`, `inst_alloc_offsets`, `edits` (every `Move` between registers and `stackN` slots, keyed by program point), `num_spillslots` and `stats`.                                                                       |
| regalloc2 trace log        | The `trace-log` feature of `cranelift-codegen` and a `log::Log`                                                                                                  | A free-text dump of live ranges, bundles, spill weights and eviction decisions: about 700 lines for a 4-instruction function and 3500 for a 33-instruction one.                                                          |

Notes on `regalloc2::Output`:

- `Ctx` has private fields, but `output` is public and is not cleared after the
  call. The pinned `=0.136.2` uses the same `regalloc2` type.
- Program points in `edits` and indexes in `inst_alloc_offsets` are vcode
  instruction indexes. The vcode is consumed during emit, so no public mapping
  leads from a vcode index to a CLIF instruction or a disassembly line. Counting
  lines per block aligns them, but a lowered instruction that expands to several
  machine instructions breaks the count.
- `stats` (`splits`, `evict_bundle_count`, `spill_bundle_count`, `edits_count`)
  is filled by the default backtracking allocator and is all zeros for
  `regalloc_algorithm = "single_pass"`. `edits` and `allocs` are filled by both.
- `ctx.output` has no live ranges, bundles or interference. Those live in
  private fields of `Ctx` and in `ion::Env`.

## What each view can show

| Feature                         | Available                 | Source                         | Limits                                                                                                                                                                                                                          |
| ------------------------------- | ------------------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Virtual to physical assignments | yes                       | `value_labels_ranges`          | Per CLIF value and code-offset range, including moves between a register and a stack slot. Values the optimizer removes do not appear. The CLIF text parser sets no labels, so we set one per value.                            |
| Spill and reload highlighting   | yes, to instruction level | `value_labels_ranges`, `edits` | A `CFAOffset` range says a value is in memory, and its transition points are the spill and reload. Pinning one to a CLIF line goes through `get_srclocs_sorted`. The vcode text cannot tell a spill `mov` from any other `mov`. |
| Allocation moves                | yes, best effort          | `edits`, vcode text            | The count and from/to allocation of every move are exact. Showing them against the disassembly relies on the line-counting alignment above.                                                                                     |
| Register pressure               | yes, derived              | `value_labels_ranges`          | The count of simultaneously live registers per code offset. It covers only labelled values, so it undercounts temporaries the lowering creates. `num_spillslots` and `stats` give totals.                                       |
| Live ranges                     | partly                    | `value_labels_ranges`          | The assigned location range of each CLIF value, not regalloc2's live ranges. No bundles or split points.                                                                                                                        |
| Interference                    | no                        | none                           | No public type exposes it. The trace log mentions conflicts, but it is unstructured, needs a global logger and a non-default feature, and has no stable format.                                                                 |
| Why a value was spilled         | no                        | none                           | The reasons (eviction by a higher spill weight, no free register) exist only in the trace text and in `stats` counters, with no link from a counter to a value.                                                                 |

## Costs

- `compile_function` bypasses `Context::compile`, so we run `Context::optimize`
  and compute the CFG and dominator tree ourselves. This matches what
  `compile_stencil` does today, but it couples us to Cranelift internals that a
  version bump can break. The `=0.136.2` pin gates it.
- `regalloc2` is a direct dependency, pinned to the version `cranelift-codegen`
  uses.
- Source locations and value labels change the `Function` we compile, so they
  are set on a clone and never on the function used for diagnostics.
- `trace-log` is a compile-time feature that slows compilation for every user,
  and its text is not API, so nothing shipped depends on it for regalloc.
