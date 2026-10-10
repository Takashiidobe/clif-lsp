# Custom requests

These requests are not part of LSP. Each takes a `textDocument` and fails with
an invalid-params error if the document is not open.

## clif/stage

Returns one function as Cranelift sees it after a pipeline stage.

Params:

- `textDocument`: the document.
- `function`: the function name, for example `%main`. Defaults to the first
  function in the file.
- `stage`: one of the values below.

Stages:

- `parsed`: the function as `cranelift-reader` prints it.
- `optimized`: the function after Cranelift's optimizer.
- `vcode`: the lowered virtual-register code for the target.

Result: `{ function, stage, text }`.

`optimized` and `vcode` use the file's `target` line, or x86_64 when there is
none. If `cranelift-reader` rejects the file, the error carries its message and line.

## clif/controlFlowGraph

Returns the control flow graph of every function.

Result: `{ functions: [{ name, entry, blocks, dot }] }`.

- `blocks` lists `{ name, successors, reachable }`. `successors` holds indexes
  into `blocks`.
- `entry` is the index of the entry block, or null for an empty function.
- `dot` is the same graph as Graphviz DOT text.

## clif/analysis

Returns dominance and liveness for every function.

Result: `{ functions: [{ name, blocks, values }] }`.

- `blocks` lists `{ name, immediateDominator, predecessors }`. The dominator is
  null for the entry block and for unreachable blocks.
- `values` lists `{ name, definedIn, liveIn, liveOut }`, where the last two are
  block names.

## clif/view

Returns a named view of one function as a self-describing document. Clients
render the sections generically, so a new view needs no new client code.

Params:

- `textDocument`: the document.
- `view`: the view name. One of `cfg`, `analysis`, `stage`, `allocators`,
  `regalloc`, `provenance`, `egraph`.
- `function`: the function name, for example `%main`. Defaults to the first
  function in the file.
- `args`: an optional object of view-specific arguments.

Result: `{ schema, view, function, title, sections }`.

- `schema` is the schema version, currently 1. It changes only for
  incompatible changes. New fields and new section kinds can appear without a
  bump, so clients must ignore unknown fields and skip sections of an unknown
  kind.
- `sections` is an ordered list. Each section has a `kind` that says how to
  render it and a `title`.

Section kinds:

- `text`: `{ text }`, preformatted plain text.
- `html`: `{ html }`, a complete standalone HTML page with inline CSS, SVG and
  script, no external requests. Save it to a file and open it in a browser.
- `table`: `{ columns, rows }`. `columns` is a list of headings and each row is
  a list of strings, one per column.
- `tree`: `{ nodes }`, where each node is `{ label, children }` and `children`
  is a list of nodes.
- `diff`: `{ from, to, text }`. `from` and `to` label the two sides and `text`
  is a unified diff.

Views:

- `cfg`: an `html` section with the control flow graph, one box per block
  listing its instructions, and a `table` section with one row per block
  (`block`, `successors`, `reachable`).
- `analysis`: a `tree` section with the dominator tree rooted at the entry
  block, a `table` of blocks (`block`, `immediate dominator`, `predecessors`)
  and a `table` of values (`value`, `defined in`, `live in`, `live out`).
- `stage`: one `text` section with the function after a pipeline stage. Needs
  `args.stage`, one of the `clif/stage` stages. Uses the file's `target` line,
  or x86_64 without one.
- `allocators`: compiles the function with the `backtracking` and `single_pass`
  register allocators, keeping the file's other settings. A `table` section
  gives code bytes, instructions, moves and stack accesses for each, and a
  `diff` section gives the unified diff of the two vcode texts. Moves and stack
  accesses are counted from the vcode text: a move is a `mov` or `fmov` line
  with no memory operand, and a stack access is a line that addresses `%rsp`,
  `%rbp` or `sp`. Callee-saved register saves and restores count as stack
  accesses.
- `regalloc`: where the register allocator put each value. The function is
  optimized first, so the value numbers are those of the optimized CLIF, and it
  is compiled with the file's allocator setting. Four sections: a `table` of
  totals (`values placed`, `max register pressure`, `spills`, `reloads`,
  `moves`, `spill slots`, `regalloc2 edits`), a `table` of assignments
  (`value`, `location`, `event`, `from`, `until`), a `table` of register
  pressure (`instruction`, `registers`, `values`), and an `html` allocation
  map. Spills are red, reloads green and moves yellow.

  Register pressure is the number of values held in registers at an
  instruction, counting both its operands and its results. It only counts
  values that carry a location, so it undercounts temporaries the lowering
  creates. The allocation map has one row per instruction, grouped by block
  with its successors, and one column per value. A cell shows the value's
  location where it starts and `|` while it stays there; blue is a register,
  purple the stack, and the last column is the pressure as a bar with the peak
  in bold. These are the locations the allocator
  assigned to each CLIF value, not regalloc2's own live ranges; interference
  and the reason for a spill are not available from the public API.

  A `location` is a register (`%rsi`) or a stack slot (`[cfa-80]`). `event` is
  `def` for a value's first location, `spill` when it moves from a register to
  the stack, `reload` for the reverse, and `move` for any other change. `from`
  and `until` are CLIF instructions, or `parameter` for a block parameter's
  first location. A spill, reload or move is placed at the first instruction
  where the new location holds. The data comes from Cranelift's value-label
  debug ranges, so only values that survive optimization appear, and it says
  where a value lives, not why the allocator chose it.
- `provenance`: which input instruction each optimized instruction came from.
  Every input instruction is tagged with a unique source location, the function
  is optimized, and the tags are read back. Four sections: a `table` of counts
  (`input instructions`, `unchanged`, `changed`, `new, no provenance
  available`, `no longer present`), an `html` page with the input instructions
  on the left, the optimized ones on the right and a curve wherever a tag
  survived (red: no longer present, yellow: changed, green: new), a `table` of
  optimized instructions (`block`, `instruction`, `status`, `origin`), and a
  `table` of input instructions whose tag no longer appears.

  `status` is `unchanged` or `changed` when the tag survived, comparing the
  instruction text before and after, and `new` when the optimizer created the
  instruction. Cranelift gives instructions it creates no source location, so a
  new instruction says `no provenance available` rather than a guess. An input
  instruction that is `no longer present` was deleted, merged into another
  (value numbering), or replaced by a new instruction (constant folding,
  strength reduction); the public API does not say which, or which rule fired.

- `egraph`: the equivalent forms the optimizer's e-graph considered for each
  input value, and which one it extracted. Three sections: a `table` of counts
  (`input values`, `with alternatives`, `rewritten by the extractor`,
  `rewrite rule invocations`, `rewrite rule results`), an `html` page with one
  card per input instruction listing its forms (for `imul v0, 8`: `imul v0, 8`
  at cost 11 and `ishl v0, 3` at cost 4, the second highlighted as chosen), and
  a `table` of the same forms (`input`, `form`, `cost`, `chosen`, `rule`).

  The data is parsed from Cranelift's trace log, captured through the `log`
  crate with the `trace-log` feature of `cranelift-codegen`. See
  [egraph-visibility.md](egraph-visibility.md). `cost` is the extractor's own
  cost model, not a measurement. `rule` is the ISLE file and line of the rule
  that produced the form; the rule text is not available. Forms are printed
  from Cranelift's debug output, so they have no types and constants are shown
  inline. Only `opt_level=speed` and `speed_and_size` run the egraph pass;
  otherwise the page says there is no data. Instructions with side effects or
  several results, such as calls, are not part of the model. The log format is
  not an API, so the view is tied to the pinned Cranelift version.

An unknown view, function or stage, or a missing required argument, fails with
an invalid-params error.
