# Features

This lists everything the server does. Positions and ranges follow LSP.

## Diagnostics

Diagnostics update on every edit. They come in layers, and a later layer only
runs when the earlier ones are clean:

1. **Syntax errors**: bad tokens, unbalanced brackets, unclosed function bodies,
   misplaced instructions, and unknown opcodes. The parser recovers, so one
   error does not hide the rest of the file.
2. **Name resolution**: undefined values, blocks, and declarations such as `ss0`
   or `fn0`. These are warnings while syntax errors exist and errors otherwise.
3. **Reader and verifier errors**: whatever `cranelift-reader` rejects, then
   what the Cranelift verifier reports, placed on the instruction, block, or
   declaration it names. The reader stops at its first error, so verifier errors
   only appear once the file reads.

Blocks that no path from the entry reaches get an "unreachable" hint, which
editors usually show faded.

## Navigation

- **Go to definition** works on values, blocks, declarations, and function
  names.
- **Find references** lists every use, with or without the declaration.
- **Rename** works on values, blocks, declarations, and function names. It
  rejects names with the wrong shape (`v3`, `block2`, and so on), names that are
  already taken, and declarations that change their prefix (`ss0` to `gv0`).
  Values, blocks, and declarations are scoped to their function.
- **Document highlight** marks the definition and uses of the name under the
  cursor.
- **Document symbols** give an outline: functions, with their declarations and
  blocks nested inside.
- **Call hierarchy** shows incoming and outgoing calls between functions in the
  file. Callees defined elsewhere appear as external functions.

## Hover

Hover shows:

- **Opcodes**: the signature, a description, and the flags it sets.
- **Values**: the defining instruction or block parameter, the inferred type,
  the block it is defined in, and the blocks it is live into and out of.
- **Blocks**: the immediate dominator and the predecessors.
- **Declarations and function names**: the declaration, or `external function`
  for a name with no body in the file.
- **Types, condition codes, trap codes, and memory flags**: what they mean.

## Completion

Completion triggers on `.` and `%`, and also works on request. It offers:

- Opcodes at the start of an instruction. If the client supports snippets, an
  opcode inserts its operands as tab stops: values as `v0`, `v1`, ..., immediates
  as `0`, `Cond` as a choice list, blocks, `fn`, `sig`, `ss` and `gv` operands as
  `block0`, `fn0` and so on. Opcodes with a controlling type, such as `iconst`
  and `load`, get a `.i32` suffix tab stop first. Optional memory flags are left
  out. Opcodes whose operands can't be filled this way, such as `br_table` and
  `try_call`, insert the bare name.
- Types after `.` on an opcode, as in `iconst.`.
- Condition codes, trap codes, and memory flags where an opcode expects them.
- Values, blocks, declarations, and function names that are in scope in operand
  position.
- Types inside the parameter and return lists of `function` and `block` headers.

## Signature help

Signature help triggers on space, `,`, and `(`. Inside an instruction it shows
the opcode's signature with the current operand highlighted. Inside a call such
as `call fn0(`, it shows the callee's signature taken from the `fn` and `sig`
declarations.

## Inlay hints

- **Result types**: `: i32` after a result whose type the instruction does not
  already spell out. Types are inferred by `cranelift-reader`, so they need a
  file that reads.
- **Branch arguments**: the target block's parameter name before each argument,
  as in `jump block1(x: v2)` when `block1(v1: i32)` declares the parameter.

## Code actions

Quick fixes attach to the diagnostic they solve:

- **Change to `name`**: replaces an unknown opcode with the closest opcode
  names. The nearest one is preferred.
- **Create block / Declare**: adds a stub for an undefined block, `ss`, `gv`, or
  `sig`. Stubs are `trap user1`, `explicit_slot 8`, `vmctx`, and `() system_v`.
  Edit them after.
- **Insert `return` / Insert `trap user1`**: ends a block that has no
  terminator. `return` is only offered when the function returns nothing.
- **Add block parameter `v: type`**: for a value used but never defined in a
  block other than the entry block. The type comes from the signature for
  `return`, from the target block's parameter for a branch argument, or from a
  block parameter used in the same `iadd`-style instruction. If none applies,
  the action is not offered. Branches into the block are not updated, so the
  verifier will report the missing arguments afterwards.

Refactor:

- **Remove unused `name`**: deletes a declaration that nothing uses.
- **Collapse `brif` into `jump`**: when both targets, arguments included, are
  the same.
- **Reuse `vN` for duplicate constant**: on an `iconst` that repeats an earlier
  identical `iconst` in the same block or in a block that dominates it. Uses of
  the duplicate are renamed and its line is deleted.
- **Remove unused parameter `v` from `blockN`**: on a block parameter that
  nothing uses, outside the entry block. Deletes it from the block header and
  the matching argument from every branch into the block. The parentheses go
  too when it was the only one.
- **Move trailing instructions into new block**: splits the block before the
  instruction under the cursor with a `jump` to a new `blockN`. Not offered on
  the first instruction of a block.

## Formatting

Format document and format range both use the same fixed style:

- Blocks, `function`, and the closing `}` at column 0. Declarations and
  instructions indented four spaces. The editor's tab settings are ignored.
- One space after commas and around `=` and `->`. No space before `,`, `:`, `)`,
  or after `(`. One space before `{`. Other spacing, such as `v0+8`, is kept.
- Comment text is untouched. Comments on their own line take the indent of the
  statement after them.
- Runs of blank lines become one. Leading and trailing blank lines are removed
  and the file ends in a newline.
- Top-level `test`, `set`, and `target` lines only get trimmed and their
  whitespace collapsed.

Formatting changes whitespace only. Functions whose body is not closed are left
alone, and everything else is formatted even if it has errors.

## Folding, selection, and highlighting

- **Folding ranges** cover function bodies, blocks, and runs of comment lines.
- **Selection ranges** expand from the word under the cursor to its instruction
  or declaration, then its block, its function, and the whole file.
- **Semantic tokens** classify keywords (including opcodes), types, values,
  blocks, declarations, function names, flags and condition codes, numbers,
  strings, and comments.

## Custom requests

Four requests go beyond LSP: the optimized or lowered form of a function, the
control flow graph, dominance and liveness, and `clif/view`, which serves the
register allocation, optimizer, disassembly and use views shown in the
[README](../README.md#features). See [requests.md](requests.md).
