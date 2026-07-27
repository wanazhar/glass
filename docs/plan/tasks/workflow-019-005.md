---
id: workflow-019-005
scope: transactional workflow terminal proofs and typed outputs
status: completed
depends-on: [workflow-019-004]
---

# Objective

Require a verified terminal condition before reporting workflow completion and
extract declared outputs from bounded, typed, non-JavaScript sources.

# Contract

- A successful run includes a terminal proof with the predicate, revision, and
  bounded observed state.
- Terminal-condition failure never reports `completed` and never claims a
  rollback.
- Outputs support page URL, page title, and visible text sources only.
- Declared output types are checked against their source and extraction bytes
  are bounded by `maxExtractedBytes`.

# Verification

- Full library and integration test suites.
- Opt-in local Chrome fixture covers terminal proof and title extraction.
- `cargo fmt --all -- --check`.
- `cargo clippy --all-targets --all-features --locked -- -D warnings`.

# Delivery

Commit locally with a Conventional Commit message. Do not push, publish,
create a GitHub release, or close issue #21 from this task.
