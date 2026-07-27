---
id: workflow-019-003
scope: transactional workflow classification and retry safety
status: completed
depends-on: [workflow-019-002]
---

# Objective

Add explicit transaction classifications, idempotency-key validation, and
bounded retry policy to issue #21 without replaying an action after dispatch.

# Contract

- Steps classify effects as read-only, idempotent, conditionally idempotent,
  non-idempotent, or unknown.
- Conditional idempotency requires an idempotency key.
- Unknown and non-idempotent steps cannot request automatic retries.
- Retries are bounded by the workflow budget and only happen after a failure
  proven to occur before dispatch.
- A successful dispatch, or any failure reported after dispatch, is never
  automatically replayed.

# Verification

- Exact-path validation tests for missing keys and unsafe retry requests.
- Retry policy tests cover class, attempt, and dispatch boundaries.
- Full library and integration test suites.
- `cargo fmt --all -- --check`.
- `cargo clippy --all-targets --all-features --locked -- -D warnings`.

# Delivery

Commit locally with a Conventional Commit message. Do not push, publish,
create a GitHub release, or close issue #21 from this task.
