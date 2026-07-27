# Workflow 019-016 — semantic recorder draft foundation

## Status

Completed locally.

## Scope

Add a bounded, in-memory recorder draft model for semantic authoring. Drafts
retain role/name targets and review metadata, while typed text is represented by
an input placeholder rather than a literal value.

## Acceptance criteria

- [x] Recorder drafts use semantic role/name target metadata.
- [x] Every recorded action is marked review-required.
- [x] Typed values are stored as placeholders, not literal values.
- [x] Recorded click, text-input, and observation drafts are bounded and
  duplicate-ID checked.
- [x] Drafts can be converted into the normal workflow definition after
  explicit caller-provided inputs and terminal/output declarations.
- [x] Documentation states that the foundation does not attach to or infer from
  a browser session.

## Validation

```text
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
git diff --check
```
