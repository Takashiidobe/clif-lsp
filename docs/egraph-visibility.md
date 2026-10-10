# E-graph visibility (public API only)

Spike for `clif-lsp-ee9.2`. Question: can equivalent expressions and the
extracted choice be shown using only public `cranelift-codegen =0.136.2` API?
Everything below was run against 0.136.2 on x86_64 with `opt_level=speed`.

**Decision: go**, with a caveat about stability (below). The egraph itself is
not public, but its trace log is, and the trace log carries everything the
explorer needs.

## What is not reachable

- `cranelift_codegen::egraph` is a private module. `EgraphPass`, its `Stats`
  and the eclass tables are not exported, and `Context` keeps no copy of them
  after `optimize`.
- The output function has no eclasses left: elaboration picks one node per
  class and deletes the rest. Provenance srclocs (see `clif/view` `provenance`)
  show only which input instruction survived.

## What is reachable: the `log` facade

`Context::optimize` writes the egraph's whole life to the `log` crate. Install a
`log::Log` that records `Record::target()` and `Record::args()`, set the max
level to `Trace`, and run `optimize`.

Without the stock `trace-log` cargo feature of `cranelift-codegen`, only the
`log::trace!` calls that bypass Cranelift's own `trace!` macro are compiled into
a release build. That gives the value mapping (`optimizing inst inst1 orig
result v2 gave v8`) and a stats dump (`union: 2`, `rewrite_rule_invoked: 7`,
`remat`, `elaborate_licm_hoist`, ...), but no eclass membership and no costs.
Debug builds of the dependency have everything because `trace!` is gated on
`debug_assertions` too.

With `features = ["trace-log"]` (a public feature, not a patch) a four-line
function produces about 345 lines. For `v2 = imul v0, 8` the interesting ones
are, in order, all from target `cranelift_codegen::egraph`:

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

The `cranelift_codegen::opts::generated_code` target names the ISLE rule that
fired, as a file and line: `ISLE simplify src/opts/shifts.isle line 315` for
the multiply-to-shift rewrite, `src/opts/remat.isle line 25` for constant
rematerialization. The ISLE sources are not shipped at runtime, so the view can
cite the location but not quote the rule.

| Wanted | Reachable | Source line |
|---|---|---|
| Equivalent forms of a value (`imul x,8` and `ishl x,3`) | yes | `v8 = Union(v2, v7)` plus the defining instructions in the `egraph built` dump |
| Which form was chosen | yes | `best of union(...) = BestEntry(cost, vN)`, `elaborate: value vA -> best vB` |
| Cost of each alternative | yes | `cost of value vN = Cost::Finite(n)` |
| Which rule produced the alternative | file and line only | `ISLE simplify <file> line <n>` |
| Mapping from input value to egraph value | yes | `optimizing inst instN orig result vA gave vB` |
| Stats per pass | yes, release too | `egraph stats: Stats { ... }` |
| Alternatives that were rejected as too deep or too large | counts only | `rewrite_depth_limit`, `eclass_size_limit` |

## Constraints

1. **Not an API.** The text is `Debug` output and free-form messages. It can
   change in any release. The workspace already pins `=0.136.2`, so parsing is
   safe until the pin moves; each bump needs the fixtures to be re-run. Parse
   only lines we recognise and drop the rest, and treat an unrecognised
   format as "no egraph data" rather than an error.
2. **Needs the `trace-log` feature.** It is a stock cargo feature and also
   turns on `regalloc2/trace-log`, so regalloc's trace output appears in the
   same capture. Filter on target (`cranelift_codegen::egraph`,
   `::egraph::elaborate`, `::opts`) at record time to keep the buffer small.
3. **Process-wide logger.** `log::set_logger` can be called once and applies to
   all threads. Install one logger at startup that appends to a thread-local
   buffer only while a capture guard is alive. `Context::optimize` runs on the
   calling thread, so captures from concurrent requests do not mix. Raising
   `log::set_max_level` is also global, so keep it at `Off` outside a capture.
4. **Value numbers are egraph-internal.** `v8`, `v11` and friends are numbers
   in the temporary egraph, not CLIF values in either the input or the
   optimized function. The view must translate through the `orig result vA gave
   vB` lines and show instruction text, never raw numbers alone.
5. **Cost is Cranelift's model.** `Cost::Finite(11)` for an `imul` is the
   extractor's guess, not a measurement of the target machine.
6. **Optimizing only.** Only `opt_level=speed` or `speed_and_size` runs the
   egraph pass; at `none` the log is empty and the view says so.

## Follow-up beads

- Capture harness: thread-local `log::Log`, `trace-log` feature, a guard that
  runs `Context::optimize` and returns the filtered records.
- Parser and model: eclasses (members, defining instruction text, cost, chosen
  member, rule location), tolerant of unknown lines.
- `egraph` view and `:ClifEgraph`: an HTML page listing each class that has
  more than one member, alternatives with costs, the chosen one highlighted,
  and the rule file and line.
