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
- `optimized`: the function after Cranelift's optimizer. Needs a `target` line.
- `vcode`: the lowered virtual-register code for the target. Needs a `target`
  line.

Result: `{ function, stage, text }`.

If the file has no `target` line, `optimized` and `vcode` fail with an error
that says to add one, for example `target x86_64`. If `cranelift-reader` rejects
the file, the error carries its message and line.

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
- `view`: the view name. Currently `cfg`.
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
- `dot`: `{ dot }`, Graphviz DOT source to be laid out by the client.
- `table`: `{ columns, rows }`. `columns` is a list of headings and each row is
  a list of strings, one per column.
- `tree`: `{ nodes }`, where each node is `{ label, children }` and `children`
  is a list of nodes.
- `diff`: `{ from, to, text }`. `from` and `to` label the two sides and `text`
  is a unified diff.

Views:

- `cfg`: a `dot` section with the control flow graph and a `table` section with
  one row per block (`block`, `successors`, `reachable`).
- `analysis`: a `tree` section with the dominator tree rooted at the entry
  block, a `table` of blocks (`block`, `immediate dominator`, `predecessors`)
  and a `table` of values (`value`, `defined in`, `live in`, `live out`).
- `stage`: one `text` section with the function after a pipeline stage. Needs
  `args.stage`, one of the `clif/stage` stages, and has the same `target` line
  requirement.

An unknown view, function or stage, or a missing required argument, fails with
an invalid-params error.
