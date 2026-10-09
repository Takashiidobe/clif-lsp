# Fixtures

- `valid/`: CLIF accepted by cranelift-reader. Copied from `../clif2wasm/tests/fixtures` at commit `4ffca63218f69da84aa518a66247db9e5508978a`. `valid/lowering_errors/` is its `error/` directory (valid CLIF that clif2wasm rejects at lowering).
- `broken/`: hand-written invalid or half-typed input. The parser must recover and never panic.

Expectations are `; CHECK:` lines in each file, run by `cargo test --test fixtures`.

`tokens` mode positions are `line:byte-column`, with line 0 being the first line after the leading block of directive comments. Directive comments print as `Comment "<directive>"` so checks cannot match themselves.

The harness also runs invariants on every fixture, whatever its `mode:`: the lexer tiles the input exactly, the parser does not panic, and every span is in bounds on char boundaries. These are re-checked on every char-boundary prefix of the file.
