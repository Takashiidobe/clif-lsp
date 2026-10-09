# Project Instructions for AI Agents

This file provides instructions and context for AI coding agents working on this project.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:1105d646 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/core-concepts/sync-concepts.md for details and anti-patterns.

## Agent Context Profiles

The managed Beads block is task-tracking guidance, not permission to override repository, user, or orchestrator instructions.

- **Conservative (default)**: Use `bd` for task tracking. Do not run git commits, git pushes, or Dolt remote sync unless explicitly asked. At handoff, report changed files, validation, and suggested next commands.
- **Minimal**: Keep tool instruction files as pointers to `bd prime`; use the same conservative git policy unless active instructions say otherwise.
- **Team-maintainer**: Only when the repository explicitly opts in, agents may close beads, run quality gates, commit, and push as part of session close. A current "do not commit" or "do not push" instruction still wins.

## Session Completion

This protocol applies when ending a Beads implementation workflow. It is subordinate to explicit user, repository, and orchestrator instructions.

1. **File issues for remaining work** - Create beads for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Handle git/sync by active profile**:
   ```bash
   # Conservative/minimal/default: report status and proposed commands; wait for approval.
   git status

   # Team-maintainer opt-in only, unless current instructions forbid it:
   git pull --rebase
   git push
   git status
   ```
5. **Hand off** - Summarize changes, validation, issue status, and any blocked sync/commit/push step

**Critical rules:**
- Explicit user or orchestrator instructions override this Beads block.
- Do not commit or push without clear authority from the active profile or the current user request.
- If a required sync or push is blocked, stop and report the exact command and error.
<!-- END BEADS INTEGRATION -->


## Build & Test

```bash
cargo build
cargo test --test fixtures   # runs every tests/fixtures/**/*.clif through its mode/check directives
```

## Architecture Overview

Tolerant lexer/parser for Cranelift IR (CLIF) text, a tower-lsp-server front end, and opcode tables generated from cranelift 0.136.2.

## Conventions & Patterns

### Commits

- Commit once at the end of each bead task, after the quality gates pass and the bead is closed.
- One-line message, imperative, no body (e.g. `Add tolerant parser for function bodies`).
- This overrides the Conservative profile above for task-end commits only. Do not push unless asked.

### Testing

- No unit tests: no `#[test]`, no `#[cfg(test)]` modules, no proptest. Correctness is checked by fixtures.
- Every behavior is covered by a `.clif` fixture under `tests/fixtures/` run through the fixture harness (`cargo test --test fixtures`), which pipes the tool's output through FileCheck-style `; check:`, `; nextln:`, `; sameln:`, `; not:` directives (the `filecheck` crate, same syntax as Cranelift's own tests).
- A bead that adds or changes behavior is not done until a fixture with `check:` lines covers it.
- `; check:`, `; mode:` and other directive lines are ordinary `;` comments to the lexer/parser. Keep them as comments in the syntax tree.
