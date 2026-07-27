# Workflow 019-013 — bounded conditional steps

## Status

Completed locally.

## Scope

Add a declarative `when` predicate to workflow steps. Conditions are evaluated
once before dispatch, false conditions become explicit skipped states, and the
predicate result is retained in the run trace for offline inspection.

## Acceptance criteria

- [x] `when` is optional, validated, and represented in canonical workflow JSON.
- [x] A matched condition proceeds to the existing action runner.
- [x] An unmet condition records `skipped` without dispatching the action.
- [x] Predicate errors fail as pre-dispatch failures.
- [x] Conditional steps cannot opt into automatic repetition.
- [x] Branch decisions are included in traces and replay validation.
- [x] Focused tests cover serialization and offline trace replay.

## Validation

```text
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
git diff --check
```
