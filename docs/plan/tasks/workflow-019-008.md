---
id: workflow-019-008
scope: transactional workflow CLI surface
status: completed
depends-on: [workflow-019-007]
---

# Objective

Expose the validated workflow runner through the local CLI while preserving
the same definition and input contract as the library API.

# Contract

- `glass workflow [JSON_FILE]` accepts a wrapper with `workflow` and `inputs`,
  or a workflow definition by itself.
- Definitions and inputs are validated before dispatch.
- Results are structured JSON containing states, proof, outputs, and trace.
- No publishing or remote release action is part of this surface.

# Verification

- CLI parser coverage for the optional JSON path.
- Full library and integration test suites.
- `cargo fmt --all -- --check`.
- `cargo clippy --all-targets --all-features --locked -- -D warnings`.

# Delivery

Commit locally with a Conventional Commit message. Do not push, publish,
create a GitHub release, or close issue #21 from this task.
