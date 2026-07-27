---
id: workflow-019-001
scope: transactional workflow definition foundation
status: completed
depends-on: []
---

# Objective

Add the non-browser foundation for issue #21: a versioned workflow definition
model with typed inputs, stable step identifiers, explicit budgets and terminal
conditions, deterministic camelCase serialization, and path-aware validation.

This task must not launch Chrome, mutate a browser, add a workflow runner, or
claim support for checkpoints and resume. Those belong to later tasks.

# Context

- `docs/plan/analysis/transactional-workflows-019.md`
- `docs/action-contract.md`
- `docs/schema-compatibility.md`
- GitHub issue #21: transactional workflow runtime

# Path

- `src/browser/session/` workflow contract module and tests
- `src/browser/mod.rs` and `src/lib.rs` public exports
- `docs/` workflow contract documentation

# Required contract

The first version must support:

- `schemaVersion`, `name`, and `workflowVersion`;
- typed string/number/boolean/URL inputs with required and max-length rules;
- stable unique step IDs and a minimal declared step shape suitable for later
  execution;
- bounded `maxSteps`, `maxDurationMs`, `maxRetries`, and
  `maxExtractedBytes` budgets;
- optional preconditions and a required terminal-condition representation;
- declared output names without arbitrary JavaScript;
- deterministic serialization and canonical camelCase field names;
- typed validation errors containing a bounded JSON path and reason.

The model may initially represent actions and predicates using existing
contract types, but it must reject missing required fields, duplicate IDs,
zero/unbounded budgets, invalid input declarations, and unknown action/predicate
variants before runtime use.

# Verification

- unit tests for valid serialization and every validation failure class;
- golden JSON fixture for a minimal linear workflow;
- tests proving duplicate IDs and invalid budgets include exact paths;
- `cargo fmt --all -- --check`;
- `cargo test --all-targets --locked`;
- `cargo clippy --all-targets --all-features --locked -- -D warnings`.

# Delivery

Commit the focused implementation with a Conventional Commit message. Do not
push, publish, create a GitHub release, or close issue #21 from this task.
