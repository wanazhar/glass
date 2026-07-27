---
id: workflow-019-006
scope: transactional workflow bounded repetition control flow
status: completed
depends-on: [workflow-019-005]
---

# Objective

Add a bounded repetition primitive for issue #21 without turning control flow
into an unbounded or unsafe replay mechanism.

# Contract

- `repeat` is bounded to eight executions per declared step.
- Expanded repetitions count against `budgets.maxSteps`.
- Repetition is allowed only for retry-safe transaction classes.
- Each repetition is represented in the retained state history and cumulative
  attempt count.
- A failed repetition stops the workflow and skips later declared steps.

# Verification

- Exact-path validation for unsafe repeated steps.
- State-machine tests cover committed-to-ready repetition transitions.
- Opt-in local Chrome fixture runs a repeated read-only observation.
- Full library and integration test suites.
- `cargo fmt --all -- --check`.
- `cargo clippy --all-targets --all-features --locked -- -D warnings`.

# Delivery

Commit locally with a Conventional Commit message. Do not push, publish,
create a GitHub release, or close issue #21 from this task.
