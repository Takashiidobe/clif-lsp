# Custom requests

These requests are not part of LSP. Each takes a `textDocument` and fails with
an invalid-params error if the document is not open.

## clif/stage

Returns one function as Cranelift sees it after a pipeline stage.

Params:

- `textDocument`: the document.
- `function`: the function name, for example `%main`. Defaults to the first
  function in the file.
- `stage`: `parsed` (the function as `cranelift-reader` prints it), `optimized`
  (after Cranelift's optimizer) or `vcode` (the lowered virtual-register code).

Result: `{ function, stage, text }`.

`optimized` and `vcode` use the file's `target` line, or x86_64 when there is
none. If `cranelift-reader` rejects the file, the error carries its message and
line.

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
- `view`: one of the views below.
- `function`: the function name, for example `%main`. Defaults to the first
  function in the file.
- `args`: an optional object of view-specific arguments.

Result: `{ schema, view, function, title, sections }`.

- `schema` is the schema version, currently 1. It changes only for incompatible
  changes. New fields and section kinds can appear without a bump, so clients
  must ignore unknown fields and skip sections of an unknown kind.
- `sections` is an ordered list. Each has a `kind` that says how to render it
  and a `title`.

Section kinds:

- `text`: `{ text }`, preformatted plain text.
- `html`: `{ html }`, a standalone HTML page with inline CSS, SVG and script and
  no external requests. Save it to a file and open it in a browser.
- `table`: `{ columns, rows }`. `columns` is a list of headings and each row is
  a list of strings, one per column.
- `tree`: `{ nodes }`, where each node is `{ label, children }`.
- `diff`: `{ from, to, text }`. `from` and `to` label the two sides and `text`
  is a unified diff.

An unknown view, function or stage, or a missing required argument, fails with
an invalid-params error.

### cfg

The control flow graph. An `html` section with one box per block listing its
instructions, and a `table` with one row per block (`block`, `successors`,
`reachable`).

### analysis

Dominators and liveness. A `tree` with the dominator tree rooted at the entry
block, a `table` of blocks (`block`, `immediate dominator`, `predecessors`) and
a `table` of values (`value`, `defined in`, `live in`, `live out`).

### stage

One `text` section with the function after a pipeline stage. Needs `args.stage`,
one of the `clif/stage` stages or a `passes` step name. Uses the file's `target`
line, or x86_64 without one.

### passes

The optimizer's steps, replayed from Cranelift's `Context::optimize`: `parsed`,
`canonicalize_nans`, `unreachable_code`, `constant_phis`, `egraph`, then
`vcode`. Sections:

- A `table` (`pass`, `ran`, `lines added`, `lines removed`, `note`).
- An `html` stepper with one section per step, reachable by `#step-N` fragments,
  previous and next arrows, a strip of step names and the left and right arrow
  keys. Each step shows the function after the pass, with lines added, changed
  or removed against the previous step.

`canonicalize_nans` and `egraph` stay in the list and are marked skipped when
their flag or `opt_level` turns them off. `vcode` is not diffed.

### diff

A `diff` section between two steps. `args` takes `from` (default `parsed`) and
`to` (default `optimized`), each a step name. `optimized` is the same as
`egraph`.

### egraph

The equivalent forms the optimizer's e-graph considered for each input value,
and the one it extracted. Sections:

- A `table` of counts (`input values`, `with alternatives`,
  `rewritten by the extractor`, `rewrite rule invocations`,
  `rewrite rule results`).
- An `html` page with one card per input instruction listing its forms. For
  `imul v0, 8` that is `imul v0, 8` at cost 11 and `ishl v0, 3` at cost 4, the
  second marked as chosen.
- A `table` of the same forms (`input`, `form`, `cost`, `chosen`, `rule`).

`cost` is the extractor's own model, not a measurement. `rule` is the ISLE file
and line that produced the form, without the rule text. Forms come from
Cranelift's debug output, so they have no types and constants are shown inline.
Instructions with side effects or several results, such as calls, are not
modelled. Only `opt_level=speed` and `speed_and_size` run the egraph pass;
otherwise the page says there is no data.

The data is parsed from Cranelift's trace log, which is not an API, so the view
is tied to the pinned Cranelift version. See
[egraph-visibility.md](egraph-visibility.md).

### provenance

Which input instruction each optimized instruction came from. Sections:

- A `table` of counts (`input instructions`, `unchanged`, `changed`,
  `new, no provenance available`, `no longer present`).
- An `html` page with the input instructions on the left, the optimized ones on
  the right, and a curve wherever a tag survived. Red is no longer present,
  yellow changed, green new.
- A `table` of optimized instructions (`block`, `instruction`, `status`,
  `origin`).
- A `table` of input instructions that no longer appear.

`status` is `unchanged` or `changed` when the instruction's tag survived,
comparing its text before and after, and `new` when the optimizer created it.
Cranelift gives created instructions no source location, so a new instruction
says `no provenance available` rather than a guess. An input instruction that is
`no longer present` was deleted, merged into another, or replaced by a new one.
The public API does not say which, or which rule fired.

### regalloc

Where the register allocator put each value. The function is optimized first, so
value numbers are those of the optimized CLIF, and compiled with the file's
allocator setting. Sections:

- A `table` of totals (`values placed`, `max register pressure`, `spills`,
  `reloads`, `moves`, `spill slots`, `regalloc2 edits`).
- A `table` of assignments (`value`, `location`, `event`, `from`, `until`).
- A `table` of register pressure (`instruction`, `registers`, `values`).
- An `html` allocation map.

A `location` is a register (`%rsi`) or a stack slot (`[cfa-80]`). `event` is
`def` for a value's first location, `spill` for a move from a register to the
stack, `reload` for the reverse, and `move` for any other change. `from` and
`until` are CLIF instructions, or `parameter` for a block parameter's first
location. An event is placed at the first instruction where the new location
holds.

Register pressure is the number of values held in registers at an instruction,
counting its operands and results. It counts only values that carry a location,
so it undercounts temporaries the lowering creates.

The allocation map has one row per instruction, grouped by block with its
successors, and one column per value. A cell shows the value's location where it
starts and `|` while it stays there. Blue is a register, purple the stack.
Spills are red, reloads green and moves yellow. The last column is the pressure
as a bar, with the peak in bold.

The data is Cranelift's value-label debug ranges, so only values that survive
optimization appear, and it says where a value lives, not why. Interference and
spill reasons are not available from the public API. See
[regalloc-visibility.md](regalloc-visibility.md).

### allocators

Compares the `backtracking` and `single_pass` register allocators on the
function, keeping the file's other settings. A `table` gives code bytes,
instructions, moves and stack accesses for each, and a `diff` section gives the
unified diff of the two vcode texts.

Moves and stack accesses are counted from the vcode text. A move is a `mov` or
`fmov` line with no memory operand. A stack access is a line that addresses
`%rsp`, `%rbp` or `sp`, which includes callee-saved register saves and restores.

### live_ranges

Where each value is live, computed from the file as written. Sections:

- An `html` page with instructions down and values across (`def`, `|`, `use`).
- A `table` (`row`, `values live`, `values`) with one row per block header and
  instruction.

A value that dies at an instruction still counts as live on it.

### def_use

Where one value is defined and which instructions use it. `args` takes `value`
(required, such as `v2`) and `depth` (a positive integer, default 1). With a
depth above 1, the uses of each using instruction's results are nested under it.
Block parameters are not followed through branches. Sections: a `table`
(`value`, `block`, `instruction`), a `tree` of uses labelled
`block: instruction`, and an `html` page with the same tree.

### use_counts

The values with no uses (`value`, `block`, `definition`) and with exactly one
use (plus `used by`), as two `table` sections and an `html` page with the same
tables. A use is one operand occurrence, so `iadd v1, v1` is two uses.

### disasm

The machine code of the optimized function for the file's `target` (x86_64
without one), disassembled with Capstone. x86_64, aarch64, riscv64 and s390x are
supported; other targets fail with an error. Sections:

- A `table` (`metric`, `value`) with the target, code bytes, machine
  instructions and how many map to CLIF.
- A `table` with one row per machine instruction (`offset`, `bytes`,
  `instruction`, `CLIF`).
- An `html` page that groups the rows under their CLIF instruction.

Each CLIF instruction is tagged with a unique source location before compiling,
and each machine instruction takes the CLIF instruction whose range contains its
offset. A CLIF instruction therefore owns the code up to the next one, so the
epilogue is listed under the `return`. The `CLIF` column is filled on the first
row of each run and blank on the rest, and on code with no CLIF instruction such
as the prologue. The CLIF is the optimized function, so value numbers can differ
from the file.
