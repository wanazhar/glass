---
id: workflow-019-004
scope: transactional workflow checkpoints and resume reconciliation
status: completed
depends-on: [workflow-019-003]
---

# Objective

Add bounded workflow checkpoints and a no-dispatch resume reconciliation step
for issue #21.

# Contract

- Checkpoints are deterministic, schema-versioned, and capped at 8 KiB.
- Checkpoints contain workflow identity/hash and bounded step/route state only;
  inputs and page secrets are excluded.
- Reconciliation rejects definition mismatches and route/target/frame changes.
- Only a pending step or a pre-dispatch failure in a retry-safe class can be
  returned as the next resume step.
- Dispatched, indeterminate, and post-dispatch failure states fail closed.
- Reconciliation never dispatches browser actions.

# Verification

- Deterministic redaction and parse round-trip unit tests.
- Opt-in local Chrome fixture export and reconciliation test.
- Full library and integration test suites.
- `cargo fmt --all -- --check`.
- `cargo clippy --all-targets --all-features --locked -- -D warnings`.

# Delivery

Commit locally with a Conventional Commit message. Do not push, publish,
create a GitHub release, or close issue #21 from this task.
