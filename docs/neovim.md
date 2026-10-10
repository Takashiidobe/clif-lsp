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

## Viewing the compiled output

The custom requests are not bound to any key. This command opens the first
function in the file, after the stage you name, in a scratch buffer:

```lua
vim.api.nvim_create_user_command('ClifStage', function(opts)
  local client = vim.lsp.get_clients({ bufnr = 0, name = 'clif_lsp' })[1]
  client:request('clif/stage', {
    textDocument = vim.lsp.util.make_text_document_params(),
    stage = opts.args ~= '' and opts.args or 'optimized',
  }, function(err, result)
    if err then
      vim.notify(err.message, vim.log.levels.ERROR)
      return
    end
    vim.cmd('new')
    vim.bo.buftype, vim.bo.bufhidden = 'nofile', 'wipe'
    vim.api.nvim_buf_set_lines(0, 0, -1, false, vim.split(result.text, '\n'))
  end)
end, { nargs = '?' })
```

Run `:ClifStage` for the optimized form, or `:ClifStage vcode` for the lowered
code. The `optimized` and `vcode` stages need a `target` line in the file. See
[requests.md](requests.md).
