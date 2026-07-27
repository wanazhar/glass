# Workflow 019-011 — trace replay inspection

## Status

Completed locally.

## Scope

Make workflow traces useful for offline diagnostics without turning replay into
browser action replay. The trace now records the attempt that produced each
transition and can be replayed against a validated workflow definition to
reconstruct state records.

## Acceptance criteria

- [x] Trace events carry the attempt active at each transition.
- [x] Replay validates declared step identity and ordering.
- [x] Replay validates legal state transitions and attempt increments.
- [x] Replay performs no browser or external side effect.
- [x] A focused test covers a pre-dispatch retry followed by commit.
- [x] Workflow documentation explains the offline-only replay boundary.

## Validation

```text
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
git diff --check
```
