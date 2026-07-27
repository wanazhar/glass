# Workflow 019-012 — bounded input resolution

## Status

Completed locally.

## Scope

Use validated workflow inputs in declared action fields through a small,
non-evaluating placeholder syntax. Resolution happens before browser startup or
dispatch and remains bounded by the same action limits as literal values.

## Acceptance criteria

- [x] `${inputs.name}` placeholders resolve from declared scalar inputs.
- [x] URL, target, text, select-value, wait-condition, and related action
  strings use the same resolver.
- [x] Missing, malformed, non-scalar, and oversized substitutions fail with a
  path-aware validation error.
- [x] The runner resolves actions before creating execution records or
  dispatching browser actions.
- [x] Tests cover successful substitution and an unknown placeholder.
- [x] Documentation describes the supported syntax and pre-dispatch boundary.

## Validation

```text
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
git diff --check
```
