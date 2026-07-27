# Workflow 019-021 — TUI workflow inspection path

## Status

Completed locally.

## Scope

Add a TUI command that runs a local workflow JSON document through the shared
workflow runtime and reports bounded status, trace count, and per-step states in
the activity panel.

## Acceptance criteria

- [x] The TUI parser accepts `workflow FILE`.
- [x] Wrapper and definition-only JSON documents use the same contract as CLI.
- [x] Typed inputs are passed to the shared workflow runner.
- [x] Activity output includes workflow status, trace count, and step states.
- [x] The current page context is refreshed after the run.
- [x] Parser coverage exists for the new command.

## Validation

```text
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
git diff --check
```
