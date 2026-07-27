# Workflow 019-009 — MCP execution surface

## Status

Completed locally.

## Scope

Expose the versioned workflow contract through the MCP server without adding a
second execution implementation. The MCP tool accepts a workflow definition
and optional typed inputs, delegates validation and execution to the shared
`BrowserSession` implementation, and returns the run result including step
states, terminal proof, typed outputs, and deterministic trace data.

## Acceptance criteria

- [x] The MCP tool inventory advertises `workflow` and its JSON input shape.
- [x] The parser requires a workflow definition and defaults omitted inputs to
  an empty object.
- [x] Invalid workflow definitions and input maps return bounded tool errors.
- [x] Valid requests execute through `BrowserSession::run_workflow`.
- [x] Parser coverage exists for the workflow invocation shape.
- [x] User-facing MCP documentation describes the contract and result.

## Validation

```text
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
git diff --check
```
