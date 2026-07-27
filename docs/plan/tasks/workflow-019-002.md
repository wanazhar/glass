---
id: workflow-019-002
scope: transactional workflow step states and linear execution
status: completed
depends-on: [workflow-019-001]
---

# Objective

Add the first executable workflow surface for issue #21: a serializable step
state machine and a linear runner that uses the existing policy and batch
action runtime.

# Contract

- Every step starts at `pending` and records each legal transition.
- Dispatch boundaries distinguish `failed_before_dispatch` from
  `failed_after_dispatch`.
- A successful action records `dispatched`, `effect_observed`, `verified`,
  `outputs_extracted`, and `committed`.
- A failed step stops the workflow and marks remaining steps `skipped`.
- Step expectations use the existing bounded verification predicates.
- Workflow definitions and inputs are validated before dispatch.
- This phase keeps state in the run result; checkpoints and resume belong to
  later phases.

# Verification

- State transition unit tests, including invalid jumps.
- Full library and integration test suites.
- Opt-in local Chrome fixture test through `run_workflow`.
- `cargo fmt --all -- --check`.
- `cargo clippy --all-targets --all-features --locked -- -D warnings`.

# Delivery

Commit locally with a Conventional Commit message. Do not push, publish,
create a GitHub release, or close issue #21 from this task.
