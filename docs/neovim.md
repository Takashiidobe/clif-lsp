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
          table.insert(out, '(web page: use :ClifCfg, :ClifRegalloc, :ClifProvenance, :ClifEgraph, :ClifLiveRanges, :ClifPasses, :ClifDisasm, :ClifUseCounts or :ClifDefUse)')
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
  without one. The function defaults to the first one, and is written with its `%`, as in
  `:ClifStage vcode %main`.
- `:ClifGraph` shows each function's graph as Graphviz DOT. Save the buffer to a
  file and run `dot -Tpng graph.dot -o graph.png` to draw it. This needs
  Graphviz.
- `:ClifGraphImage [function]` draws the graph and opens the image in a split.
  It draws every function, or only the one you name, as in
  `:ClifGraphImage %main`. See [Drawing the graph](#drawing-the-graph).
- `:ClifAnalysis` shows dominators and liveness as a Lua table.
- `:ClifView view [function] [args]` shows a `clif/view` view: `cfg`,
  `analysis`, `def_use`, `use_counts`, `live_ranges`, `passes`, `diff`, `disasm`, `stage`, `allocators`,
  `regalloc`, `provenance` or `egraph`. The function
  defaults to the first one. `args` is a JSON object and needs the function
  before it, as in `:ClifView stage %main {"stage":"vcode"}`.
  `:ClifView allocators` compares the two register allocators.
  `:ClifView provenance %main` shows the same data as text only.
- `:ClifCfg [function]` opens the control flow graph as a web page, with each
  block's instructions in its box. Unlike `:ClifGraphImage` it needs no
  Graphviz. See [Register allocation](#register-allocation) for the setup.
- `:ClifRegalloc [function]` shows the register assignments, spills and reloads
  as tables and a web page. See [Register allocation](#register-allocation).
- `:ClifEgraph [function]` shows the equivalent forms the optimizer considered
  for each instruction and which it chose, as tables and a web page. See
  [Register allocation](#register-allocation) for the setup.
- `:ClifProvenance [function]` shows which input instruction each optimized
  instruction came from, and which were folded away or created, as tables and a
  web page. See [Register allocation](#register-allocation) for the setup.

## Drawing the graph

`:ClifGraphImage` needs Graphviz (`dot` on your `PATH`) and a terminal that
supports the kitty graphics protocol, such as kitty, Ghostty, or WezTerm. It
also needs [snacks.nvim](https://github.com/folke/snacks.nvim) with images
turned on, and ImageMagick:

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

## Register allocation

`:ClifRegalloc [function]` shows where the register allocator put each value:
the totals, per-value assignments and register pressure in a scratch buffer, and
a web page in your browser with a chart of instructions down and values across.
Each cell shows where the value lives, with a bar for the registers live at each
instruction. Spills are red, reloads green and moves yellow. The page is plain
HTML, so it needs no Graphviz, and the browser handles zoom and scrolling. The
same helper drives `:ClifCfg` and `:ClifProvenance` below.

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
graphical_view('ClifRegalloc', 'regalloc')
graphical_view('ClifProvenance', 'provenance')
```

The page opens through `vim.ui.open`, so with `xdg-open` on Linux.

`:ClifCfg [function]` draws the control flow graph the same way: one box per
block listing its instructions, edges below it, loops looping around the right
edge. The entry block has a blue edge and unreachable blocks are dashed.

The value numbers are those of the optimized function, so they can differ from
the file. See the `regalloc` view in [custom requests](requests.md#clifview).

`:ClifProvenance [function]` uses the same helper for the optimizer. The tables
go to the scratch buffer, and a web page opens in your browser with the input
instructions on the left and the optimized ones on the right, joined by a curve
wherever the optimizer kept an instruction's source tag. Hover a row to
highlight its link and the row at the other end. Inputs that are gone are red,
changed outputs yellow and new ones green; a new output has no curve into it
because Cranelift records no provenance for instructions it creates. The page
is plain HTML and SVG, so it needs no Graphviz, and the browser handles zoom
and scrolling.

`:ClifEgraph [function]` uses the same helper for the optimizer's e-graph. The
tables go to the scratch buffer, and a web page opens with a card per input
instruction listing its equivalent forms, their costs and the ISLE rule that
produced them. The form the extractor chose is green. Add it next to the other
`graphical_view` calls:

```lua
graphical_view('ClifEgraph', 'egraph')
```

It shows nothing useful with `set opt_level=none`, since the egraph pass does
not run.

## Live ranges

`:ClifLiveRanges [function]` shows where each value of the file is live. The
pressure table (how many values are live at each block header and instruction)
goes to the scratch buffer, and a web page opens with instructions down and
values across. A cell reads `def` where the value is defined, `|` while it is
live and `use` at its last use in a block. A value that lives out of a block
stays live to the block's end. Unlike `:ClifRegalloc` it works on the file as
written, with no `target` line needed. Add it next to the other
`graphical_view` calls:

```lua
graphical_view('ClifLiveRanges', 'live_ranges')
```

## Optimization passes

`:ClifPasses [function]` steps through what the optimizer does to a function.
The scratch buffer lists the passes with the lines each one added and removed,
and a web page opens at the input. Each step shows the function after that
pass, with added lines green, changed lines yellow and removed lines red and
struck through. Click the arrows, the names across the top, or press the left and
right arrow keys to move between steps. Each step has its own URL fragment
(`#step-3`), so the page can be bookmarked at a step. It is one file with no
Graphviz and no external requests.

The steps are `parsed`, `canonicalize_nans`, `unreachable_code`,
`constant_phis`, `egraph` and `vcode`. They are the public steps of Cranelift's
`Context::optimize`, replayed one by one, so everything the optimizer rewrites
happens in `egraph`. Use `:ClifEgraph` to see what it considered inside that
step. `canonicalize_nans` runs only with the `enable_nan_canonicalization`
flag, and `egraph` only when `opt_level` is not `none`; skipped steps stay in
the list and say why. `vcode` is machine IR, shown as is rather than diffed.
`optimized` is the same as `egraph` and works wherever a step name is wanted.

`:ClifDiff [from] [to] [function]` prints a unified diff between any two steps
in the scratch buffer. It defaults to `parsed` and `optimized`. `:ClifStage
egraph` shows one step's full text. Add them next to the other `graphical_view`
calls:

```lua
graphical_view('ClifPasses', 'passes')
graphical_view('ClifDiff', 'diff', function(fargs)
  return fargs[3], { from = fargs[1], to = fargs[2] }
end)
```

## Disassembly

`:ClifDisasm [function]` compiles the function for the file's `target` line
(x86_64 without one) and shows the machine code. The scratch buffer has a
summary and a table with one row per machine instruction: its offset, bytes,
disassembly and the CLIF instruction it came from. A web page opens with the
same rows grouped under each CLIF instruction; hover a group to outline it.

The CLIF is the optimized function, so value numbers can differ from the file.
A CLIF instruction owns every machine instruction up to the next one, so the
epilogue (`popq`, `retq`) is listed under the `return`. Instructions with no
CLIF instruction, such as the prologue, have a blank CLIF column. x86_64,
aarch64, riscv64 and s390x are supported; the disassembler is Capstone, built
into the server, so nothing needs to be installed. Add it next to the other
`graphical_view` calls:

```lua
graphical_view('ClifDisasm', 'disasm')
```

## Uses

`:ClifUseCounts [function]` lists the values with no uses and the values with
exactly one use, with the instruction that uses it. The tables go to the
scratch buffer and the same two tables open as a web page.

`:ClifDefUse [value] [function] [depth]` shows where a value is defined and the
instructions that use it. Without a value it takes the word under the cursor, so
put the cursor on `v2` and run `:ClifDefUse`. The page shows the uses as a tree
you can fold. A `depth` of 2 or more also shows the uses of those instructions'
results; the default is 1. Block parameters are not followed through branches,
so a `jump block3(v3)` ends the tree.

Add both next to the other `graphical_view` calls:

```lua
graphical_view('ClifUseCounts', 'use_counts')
graphical_view('ClifDefUse', 'def_use', function(fargs)
  local value = fargs[1] or vim.fn.expand('<cword>')
  return fargs[2], { value = value, depth = tonumber(fargs[3]) }
end)
```

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
