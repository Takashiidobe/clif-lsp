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
