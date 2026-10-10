# Neovim

This setup needs Neovim 0.11 or later and `clif-lsp` on your `PATH` (see the
[README](../README.md#install)).

## Setup

Add this to your `init.lua`:

```lua
vim.filetype.add({ extension = { clif = 'clif' } })

vim.lsp.config('clif_lsp', {
  cmd = { 'clif-lsp' },
  filetypes = { 'clif' },
  root_markers = { '.git' },
})
vim.lsp.enable('clif_lsp')
```

Open a `.clif` file and run `:checkhealth vim.lsp` to confirm the client
attached.

Neovim does not set a `clif` filetype by default, so the first line is required.
Without it the server never starts.

## Using the features

Neovim 0.11 maps these by default:

- `K` shows hover.
- `grr` lists references, `grn` renames, `gra` opens code actions.
- `gO` lists document symbols.
- `CTRL-S` in insert mode shows signature help.

Go to definition is `vim.lsp.buf.definition()`. Map it if you want a key:

```lua
vim.keymap.set('n', 'gd', vim.lsp.buf.definition)
```

Turn on inlay hints per buffer:

```lua
vim.api.nvim_create_autocmd('LspAttach', {
  callback = function(args)
    vim.lsp.inlay_hint.enable(true, { bufnr = args.buf })
  end,
})
```

## Formatting

Format the whole buffer with `vim.lsp.buf.format()`, or format on save:

```lua
vim.api.nvim_create_autocmd('BufWritePre', {
  pattern = '*.clif',
  callback = function() vim.lsp.buf.format({ async = false }) end,
})
```

The formatter always uses four-space indentation and ignores `shiftwidth`.

## Custom requests

The [custom requests](requests.md) are not bound to any key. This defines one
command for each. Results open in a scratch buffer.

```lua
local function show(text)
  vim.cmd('new')
  vim.bo.buftype, vim.bo.bufhidden = 'nofile', 'wipe'
  vim.api.nvim_buf_set_lines(0, 0, -1, false, vim.split(text, '\n'))
end

local function request(method, params, render)
  local client = vim.lsp.get_clients({ bufnr = 0, name = 'clif_lsp' })[1]
  params.textDocument = vim.lsp.util.make_text_document_params()
  client:request(method, params, function(err, result)
    if err then
      vim.notify(err.message, vim.log.levels.ERROR)
    else
      show(render(result))
    end
  end)
end

vim.api.nvim_create_user_command('ClifStage', function(o)
  local stage = o.fargs[1] or 'optimized'
  request('clif/stage', { stage = stage, ['function'] = o.fargs[2] },
    function(r) return r.text end)
end, { nargs = '*' })

vim.api.nvim_create_user_command('ClifGraph', function()
  request('clif/controlFlowGraph', {}, function(r)
    local dot = {}
    for _, f in ipairs(r.functions) do
      table.insert(dot, f.dot)
    end
    return table.concat(dot, '\n')
  end)
end, {})

vim.api.nvim_create_user_command('ClifAnalysis', function()
  request('clif/analysis', {}, vim.inspect)
end, {})

local function tree_lines(nodes, depth, out)
  for _, node in ipairs(nodes) do
    table.insert(out, string.rep('  ', depth) .. node.label)
    tree_lines(node.children, depth + 1, out)
  end
end

vim.api.nvim_create_user_command('ClifView', function(o)
  local args = o.fargs[3] and vim.json.decode(o.fargs[3]) or nil
  request('clif/view', { view = o.fargs[1], ['function'] = o.fargs[2], args = args },
    function(v)
      local out = { v.title, '' }
      for _, s in ipairs(v.sections) do
        table.insert(out, '== ' .. s.title .. ' ==')
        if s.kind == 'text' then
          table.insert(out, s.text)
        elseif s.kind == 'diff' then
          table.insert(out, s.text)
        elseif s.kind == 'html' then
          table.insert(out, '(web page: use the view\'s own command, see Views)')
        elseif s.kind == 'table' then
          table.insert(out, table.concat(s.columns, '\t'))
          for _, row in ipairs(s.rows) do
            table.insert(out, table.concat(row, '\t'))
          end
        elseif s.kind == 'tree' then
          tree_lines(s.nodes, 0, out)
        else
          table.insert(out, vim.inspect(s))
        end
        table.insert(out, '')
      end
      return table.concat(out, '\n')
    end)
end, { nargs = '+' })
```

- `:ClifStage [stage] [function]` shows `parsed`, `optimized` (the default), or
  `vcode`. `optimized` and `vcode` use the file's `target` line, or x86_64
  without one. Write the function with its `%`, as in `:ClifStage vcode %main`.
  It defaults to the first function.
- `:ClifGraph` shows each function's graph as Graphviz DOT. Save the buffer to a
  file and run `dot -Tpng graph.dot -o graph.png` to draw it. This needs
  Graphviz.
- `:ClifAnalysis` shows dominators and liveness as a Lua table.
- `:ClifView view [function] [args]` shows any `clif/view` view as text. `args`
  is a JSON object and needs the function before it, as in
  `:ClifView stage %main {"stage":"vcode"}`. Tables and trees print as text; web
  pages are skipped.

## Drawing the graph

`:ClifGraphImage [function]` draws the graph of every function, or only the one
you name, and opens the image in a split. For a graph in the browser with no
Graphviz, use `:ClifCfg` under [Views](#views).

It needs Graphviz (`dot` on your `PATH`), ImageMagick, a terminal that supports
the kitty graphics protocol (kitty, Ghostty or WezTerm), and
[snacks.nvim](https://github.com/folke/snacks.nvim) with images turned on:

```lua
{ 'folke/snacks.nvim', opts = { image = { enabled = true } } }
```

Without snacks the image opens as a binary file.

Add this command next to the others:

```lua
vim.api.nvim_create_user_command('ClifGraphImage', function(o)
  if vim.fn.executable('dot') == 0 then
    vim.notify('graphviz `dot` is not installed', vim.log.levels.ERROR)
    return
  end
  request('clif/controlFlowGraph', {}, function(r)
    for i, f in ipairs(r.functions) do
      if o.args == '' or f.name == o.args then
        local png = vim.fn.tempname() .. '-' .. i .. '.png'
        local done = vim.system({ 'dot', '-Tpng', '-o', png }, { stdin = f.dot }):wait()
        if done.code == 0 then
          vim.cmd('split ' .. vim.fn.fnameescape(png))
        else
          vim.notify(done.stderr, vim.log.levels.ERROR)
        end
      end
    end
  end)
end, { nargs = '?' })
```

This also requires `request` to skip the scratch buffer when its render function
returns nothing, so change the callback in `request` to:

```lua
local text = render(result)
if text then show(text) end
```

For `tests/fixtures/analysis/diamond.clif`, `:ClifGraphImage` opens this:

![Control flow graph of the diamond fixture](images/diamond-cfg.png)

## Views

Each view puts its tables in a scratch buffer and opens its web page in your
browser through `vim.ui.open` (`xdg-open` on Linux). The pages are plain HTML,
so they need no Graphviz and the browser handles zoom and scrolling. See the
[README](../README.md#features) for what each one looks like.

Add this helper and one line per view to your `init.lua`, after `request`:

```lua
local function graphical_view(command, view, request_of)
  vim.api.nvim_create_user_command(command, function(o)
    local fn, args = o.fargs[1], nil
    if request_of then
      fn, args = request_of(o.fargs)
    end
    request('clif/view', { view = view, ['function'] = fn, args = args }, function(v)
      local out = { v.title, '' }
      for _, s in ipairs(v.sections) do
        if s.kind == 'table' then
          table.insert(out, '== ' .. s.title .. ' ==')
          table.insert(out, table.concat(s.columns, '\t'))
          for _, row in ipairs(s.rows) do
            table.insert(out, table.concat(row, '\t'))
          end
          table.insert(out, '')
        elseif s.kind == 'diff' then
          table.insert(out, s.text)
        elseif s.kind == 'html' then
          local page = vim.fn.tempname() .. '.html'
          vim.fn.writefile(vim.split(s.html, '\n'), page)
          vim.ui.open(page)
        end
      end
      return table.concat(out, '\n')
    end)
  end, { nargs = '*' })
end

graphical_view('ClifCfg', 'cfg')
graphical_view('ClifPasses', 'passes')
graphical_view('ClifEgraph', 'egraph')
graphical_view('ClifProvenance', 'provenance')
graphical_view('ClifRegalloc', 'regalloc')
graphical_view('ClifLiveRanges', 'live_ranges')
graphical_view('ClifDisasm', 'disasm')
graphical_view('ClifUseCounts', 'use_counts')
graphical_view('ClifDefUse', 'def_use', function(fargs)
  local value = fargs[1] or vim.fn.expand('<cword>')
  return fargs[2], { value = value, depth = tonumber(fargs[3]) }
end)
graphical_view('ClifDiff', 'diff', function(fargs)
  return fargs[3], { from = fargs[1], to = fargs[2] }
end)
```

All of them take an optional function name, as in `:ClifCfg %main`, and default
to the first function. `:ClifCfg`, `:ClifEgraph`, `:ClifProvenance`,
`:ClifRegalloc`, `:ClifLiveRanges`, `:ClifDisasm` and `:ClifUseCounts` take
nothing else.

### Control flow graph

`:ClifCfg` draws one box per block listing its instructions, with edges below
and loops looping around the right edge. The entry block has a blue edge and
unreachable blocks are dashed.

### Optimization passes

`:ClifPasses` steps through what the optimizer does to a function. The scratch
buffer lists the passes with the lines each added and removed, and the page
opens at the input. Added lines are green, changed lines yellow and removed
lines red and struck through. Click the arrows or step names, or press the left
and right arrow keys, to move between steps. Each step has its own URL fragment
(`#step-3`), so you can bookmark one.

The steps are `parsed`, `canonicalize_nans`, `unreachable_code`,
`constant_phis`, `egraph` and `vcode`. Everything the optimizer rewrites happens
in `egraph`; use `:ClifEgraph` to see what it considered there.
`canonicalize_nans` runs only with the `enable_nan_canonicalization` flag, and
`egraph` only when `opt_level` is not `none`. Skipped steps stay in the list and
say why. `vcode` is shown as is, not diffed. `optimized` means `egraph` wherever
a step name is accepted.

`:ClifDiff [from] [to] [function]` prints a unified diff between any two steps
in the scratch buffer. It defaults to `parsed` and `optimized`.
`:ClifStage egraph` shows one step's full text.

### E-graph

`:ClifEgraph` shows a card per input instruction with its equivalent forms,
their costs and the ISLE rule that produced them. The form the extractor chose
is green. It shows nothing useful with `set opt_level=none`, since the egraph
pass does not run.

### Provenance

`:ClifProvenance` shows the input instructions on the left and the optimized
ones on the right, joined by a curve wherever the optimizer kept an
instruction's source tag. Hover a row to highlight its link and the row at the
other end. Inputs that are gone are red, changed outputs yellow and new ones
green. A new output has no curve into it, because Cranelift records no
provenance for instructions it creates.

### Register allocation

`:ClifRegalloc` shows where the allocator put each value. The scratch buffer has
the totals, per-value assignments and register pressure, and the page has a
chart with instructions down and values across. Each cell shows where the value
lives, with a bar for the registers live at each instruction. Spills are red,
reloads green and moves yellow.

The value numbers are those of the optimized function, so they can differ from
the file.

### Live ranges

`:ClifLiveRanges` shows where each value is live. The scratch buffer has the
pressure table (how many values are live at each block header and instruction),
and the page has instructions down and values across. A cell reads `def` where
the value is defined, `|` while it is live and `use` at its last use in a block.
A value that lives out of a block stays live to the block's end. Unlike
`:ClifRegalloc`, it works on the file as written and needs no `target` line.

### Disassembly

`:ClifDisasm` compiles the function for the file's `target` line (x86_64 without
one) and shows the machine code. The scratch buffer has a summary and a table
with one row per machine instruction: offset, bytes, disassembly and the CLIF
instruction it came from. The page groups the same rows under each CLIF
instruction; hover a group to outline it.

The CLIF is the optimized function, so value numbers can differ from the file. A
CLIF instruction owns every machine instruction up to the next one, so the
epilogue (`popq`, `retq`) is listed under the `return`. Instructions with no
CLIF instruction, such as the prologue, have a blank CLIF column. x86_64,
aarch64, riscv64 and s390x are supported. The disassembler is built into the
server, so nothing needs to be installed.

### Uses

`:ClifUseCounts` lists the values with no uses and the values with exactly one
use, with the instruction that uses it.

`:ClifDefUse [value] [function] [depth]` shows where a value is defined and the
instructions that use it as a tree you can fold. Without a value it takes the
word under the cursor, so put the cursor on `v2` and run `:ClifDefUse`. A
`depth` of 2 or more also shows the uses of those instructions' results; the
default is 1. Block parameters are not followed through branches, so a
`jump block3(v3)` ends the tree.

## Trying it on the fixtures

The repository's test fixtures make good sample files. Build the server with
`cargo build` and point `cmd` at `target/debug/clif-lsp` to run it without
installing:

```lua
cmd = { '/path/to/clif-lsp/target/debug/clif-lsp' },
```

Then open a fixture and run the command in the table.

| Fixture                          | Try                                                 |
| -------------------------------- | --------------------------------------------------- |
| `valid/branch_i32.clif`          | `K` on `brif` or `v2` for hover. `:ClifGraph`.      |
| `analysis/diamond.clif`          | `:ClifAnalysis`, then `K` on a block name.          |
| `cfg/graph.clif`                 | `:ClifGraph`. One block is unreachable and faded.   |
| `stages/views.clif`              | `:ClifStage`, `:ClifStage vcode`.                   |
| `view/regalloc.clif`             | `:ClifRegalloc %pressure` for a spill and reloads.  |
| `view/egraph_view.clif`          | `:ClifEgraph %demo` for rewritten forms.            |
| `view/live_ranges.clif`          | `:ClifLiveRanges` for a diamond of live ranges.     |
| `view/passes.clif`               | `:ClifPasses`, `:ClifDiff parsed egraph %f`.        |
| `view/disasm.clif`               | `:ClifDisasm %f` for code mapped to CLIF.           |
| `view/def_use.clif`              | `:ClifDefUse v2 %f 2` for a two-level use tree.     |
| `view/use_counts.clif`           | `:ClifUseCounts` for dead and single-use values.    |
| `calls/hierarchy.clif`           | `:lua vim.lsp.buf.incoming_calls()` on a name.      |
| `actions/unknown_opcode.clif`    | `gra` on the bad opcode.                            |
| `actions/undefined_names.clif`   | `gra` on an undefined value, block, or declaration. |
| `broken/missing_brace.clif`      | Diagnostics on an unclosed function.                |
| `diagnostics/verifier.clif`      | Verifier errors on instructions.                    |
| `format/indent_and_spacing.clif` | `:lua vim.lsp.buf.format()`, then `u` to compare.   |

The `; mode:`, `; check:`, and `; lsp:` lines at the top of a fixture are plain
comments. The server ignores them, and the formatter leaves their text alone.
