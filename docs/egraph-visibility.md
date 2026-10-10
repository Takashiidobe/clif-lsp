# E-graph data from the public API

The `egraph` view shows the forms the optimizer considered and the one it chose.
This note covers where that data comes from and what limits it. It was checked
against `cranelift-codegen =0.136.2` on x86_64 with `opt_level=speed`.

## What is not reachable

- `cranelift_codegen::egraph` is private. `EgraphPass`, its `Stats` and the
  eclass tables are not exported, and `Context` keeps no copy after `optimize`.
- The optimized function has no eclasses left. Elaboration picks one node per
  class and deletes the rest.

## What is reachable: the trace log

`Context::optimize` writes the egraph's whole life to the `log` crate. The
server installs a `log::Log` that records each record's target and message, sets
the max level to `Trace`, and runs `optimize`.

Without the `trace-log` feature of `cranelift-codegen`, a release build logs
only the value mapping (`optimizing inst inst1 orig result v2 gave v8`) and a
stats dump. With it, a four-line function produces about 345 lines. For
`v2 = imul v0, 8` the useful ones are, in order, from the target
`cranelift_codegen::egraph`:

```
Processing inst inst1: v2 = imul.i64 v0, v1  ; v1 = 8
Calling into ISLE with original value v2
  -> returned from ISLE: v2 -> [v7]
 -> union: now v8
egraph built:
 -> v8 = Union(v2, v7)
 -> v7 = Result(inst7, 0)
  -> inst7 = Binary { opcode: Ishl, args: [v0, v6] }
 -> v6 = Result(inst6, 0)
  -> inst6 = UnaryImm { opcode: Iconst, imm: Imm64(3) }
```

and from `cranelift_codegen::egraph::elaborate`:

```
 -> cost of value v2 = Cost::Finite(11)
 -> cost of value v7 = Cost::Finite(4)
 -> best of union(BestEntry(Cost::Finite(11), v2), BestEntry(Cost::Finite(4), v7)) = BestEntry(Cost::Finite(4), v7)
elaborate: value v11 -> best v10
```

The target `cranelift_codegen::opts::generated_code` names the ISLE rule that
fired as a file and line, such as `ISLE simplify src/opts/shifts.isle line 315`.
The ISLE sources are not shipped, so the view cites the location and cannot
quote the rule.

| Wanted                                  | Reachable          | Source line                                                                    |
| --------------------------------------- | ------------------ | ------------------------------------------------------------------------------ |
| Equivalent forms of a value             | yes                | `v8 = Union(v2, v7)` plus the defining instructions in the `egraph built` dump |
| Which form was chosen                   | yes                | `best of union(...) = BestEntry(cost, vN)`, `elaborate: value vA -> best vB`   |
| Cost of each alternative                | yes                | `cost of value vN = Cost::Finite(n)`                                           |
| Which rule produced the alternative     | file and line only | `ISLE simplify <file> line <n>`                                                |
| Input value to egraph value             | yes                | `optimizing inst instN orig result vA gave vB`                                 |
| Stats per pass                          | yes, release too   | `egraph stats: Stats { ... }`                                                  |
| Alternatives rejected as too deep/large | counts only        | `rewrite_depth_limit`, `eclass_size_limit`                                     |

## Limits

- **The log is not an API.** The text is `Debug` output and free-form messages,
  and can change in any release. The parser keeps only lines it recognizes, and
  treats an unrecognized format as "no egraph data". Re-run the fixtures when
  the pinned Cranelift version moves.
- **It needs the `trace-log` feature.** That also turns on
  `regalloc2/trace-log`, so regalloc output lands in the same capture. The
  logger filters on target (`cranelift_codegen::egraph`, `::egraph::elaborate`,
  `::opts`) to keep the buffer small.
- **The logger is process-wide.** `log::set_logger` runs once and applies to all
  threads, so the server installs one logger that appends to a thread-local
  buffer only while a capture guard is alive. `Context::optimize` runs on the
  calling thread, so concurrent requests do not mix. The max level stays `Off`
  outside a capture.
- **Value numbers are egraph-internal.** `v8` and `v11` belong to the temporary
  egraph, not to the input or optimized function. The view translates them
  through the `orig result vA gave vB` lines and shows instruction text.
- **Cost is Cranelift's model.** `Cost::Finite(11)` for an `imul` is the
  extractor's estimate, not a measurement of the target.
- **Only optimizing builds run the pass.** At `opt_level=none` the log is empty
  and the view says so.
