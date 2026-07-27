# Workflow 019-017 — safe workflow suffix resume

## Status

Completed locally.

## Scope

Provide a library operation that combines checkpoint reconciliation with safe
execution of the pending workflow suffix. The committed prefix remains
inspection-only and is never replayed.

## Acceptance criteria

- [x] Resume reconciles workflow identity, definition hash, route, target,
  frame, title, and step state before dispatch.
- [x] Unsafe post-dispatch, indeterminate, and mismatched checkpoints remain
  rejected by the existing reconciliation contract.
- [x] Resume executes only steps at and after the reconciled safe boundary.
- [x] Already-complete checkpoints are rejected rather than run again.
- [x] The operation reuses the existing workflow runner and validation path.
- [x] Documentation states the no-replay boundary.

## Validation

```text
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
git diff --check
```
