# Workflow 019-019 — CLI checkpoint resume

## Status

Completed locally.

## Scope

Expose safe workflow suffix resume through a dedicated CLI command using the
same definition, checkpoint, input, and library contracts as MCP.

## Acceptance criteria

- [x] `workflow-resume WORKFLOW CHECKPOINT` is parsed with optional inputs.
- [x] The CLI deserializes both workflow and checkpoint through typed contracts.
- [x] Resume delegates to `BrowserSession::resume_workflow`.
- [x] CLI documentation states the no-replay and fail-closed boundaries.
- [x] Parser coverage exists for all command paths.

## Validation

```text
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
git diff --check
```
