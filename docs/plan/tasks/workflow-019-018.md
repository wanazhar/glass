# Workflow 019-018 — MCP checkpoint resume

## Status

Completed locally.

## Scope

Expose safe workflow resume through the existing MCP `workflow` tool by
accepting an optional serialized workflow checkpoint.

## Acceptance criteria

- [x] The MCP schema advertises the optional checkpoint object.
- [x] The parser preserves omitted checkpoints as normal runs.
- [x] Supplied checkpoints are deserialized through the typed checkpoint
  contract.
- [x] Resume delegates to `BrowserSession::resume_workflow` after validation.
- [x] MCP documentation states that only the safe pending suffix executes.

## Validation

```text
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
git diff --check
```
