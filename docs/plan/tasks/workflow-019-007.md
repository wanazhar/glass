---
id: workflow-019-007
scope: transactional workflow deterministic traces
status: completed
depends-on: [workflow-019-006]
---

# Objective

Add a bounded, deterministic trace of workflow state transitions for replay
inspection and future TUI/MCP integrations.

# Contract

- Trace events are ordered with contiguous sequence numbers.
- Events contain step identity, state, and attempt count only.
- Trace size is capped at 2,048 events.
- Trace validation does not require page contents or input values.

# Verification

- Unit tests build and validate a committed step trace.
- Full library and integration test suites.
- `cargo fmt --all -- --check`.
- `cargo clippy --all-targets --all-features --locked -- -D warnings`.

# Delivery

Commit locally with a Conventional Commit message. Do not push, publish,
create a GitHub release, or close issue #21 from this task.
