---
id: native-engine-002
scope: glass-browser/native-engine/dom
status: done
depends-on: [native-engine-001]
---

# Native semantic DOM and revision-bound locators

## Objective

Implement the first Phase 2 semantic DOM slice from issue #40:

- expose bounded element names and attributes from the arena;
- project supported semantic roles, accessible names, control state, and
  revision-bound native references;
- resolve explicit semantic locators (`ref`, `id`, `role`, `name`, and `text`)
  to exactly one current element; and
- fail explicitly for malformed, missing, duplicate, stale, or non-element
  targets.

This slice must not add CSS selectors, layout, hit testing, raw form-value
evidence, JavaScript, network access, or a second mutable DOM representation.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/browser.md`
- `docs/INDEX.md`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/error.rs`

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/error.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`

## Contract

Native semantic references are formatted as `ref=r<revision>:n<arena-index>`.
The current revision must match during resolution. Supported roles are limited
to the documented Phase 2 set. Accessible names use bounded explicit labels
and element text; values are not projected. A locator must resolve exactly one
current element or return a typed native error.

## Verification

```console
cargo fmt --all -- --check
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
cargo clippy -p glass-browser --features native-engine --all-targets --locked -- -D warnings
```
