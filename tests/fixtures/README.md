# Fixtures

- `valid/`: CLIF accepted by cranelift-reader. Copied from `../clif2wasm/tests/fixtures` at commit `4ffca63218f69da84aa518a66247db9e5508978a`. `valid/lowering_errors/` is its `error/` directory (valid CLIF that clif2wasm rejects at lowering).
- `broken/`: hand-written invalid or half-typed input. The parser must recover and never panic.

Expectations are `; CHECK:` lines in each file, run by `cargo test --test fixtures`.

`tokens` mode positions are `line:byte-column`, with line 0 being the first line after the leading block of directive comments. Directive comments print as `Comment "<directive>"` so checks cannot match themselves.

The harness also runs invariants on every fixture, whatever its `mode:`: the lexer tiles the input exactly, the parser does not panic, and every span is in bounds on char boundaries. These are re-checked on every char-boundary prefix of the file.

Against `cranelift-reader` (0.136.2): if we report any syntax error on a file, the reader must reject it too. For files under `valid/` the reader must accept, we must report no errors, and function, block and instruction counts must match.

Whenever the reader accepts a file, name resolution must report no problems for it.

`document` mode exercises the document store. `at` carets print the LSP UTF-16 column for a byte position and the byte column it maps back to. `edit L:C-L:C text` carets (LSP coordinates, `\n` for newline) are applied in order to the file with directive and caret lines removed, then the text and parser error count are printed.

`lsp` mode runs the real server in-process over a tokio duplex stream (`tests/harness/lsp.rs`). It sends `initialize`/`initialized`, then each `; lsp:` line in order, then `shutdown` and `exit`, and prints one transcript line per step. The document is the file with directive and caret lines removed, so positions in `lsp:` commands ignore the header. Commands: `open`, `change L:C-L:C text`, `close`, and `request <method> [json params]` (a `textDocument` is added for `textDocument/*` methods). Requests print `<method> -> <json result>` or `error <code> <message>`.

`await <method>` waits for a server notification (for example `textDocument/publishDiagnostics`) and prints `<method> <- <json params>`. Notifications that arrive earlier are queued and returned in order.
