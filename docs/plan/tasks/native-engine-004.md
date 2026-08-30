---
id: native-engine-004
scope: glass-browser/native-engine/forms
status: done
depends-on: [native-engine-003]
---

# Native single-select form controls

## Objective

Implement the next bounded Phase 2 form-control slice:

- normalize deterministic default selection for native `select`/`option`
  controls;
- expose option selection through the semantic projection without exposing
  raw form values;
- accept semantic clicks on options and update one single-select group;
- emit revisioned bounded effects through the existing interaction owner; and
- reject disabled, detached, unsupported, and multi-select targets before
  mutation.

This task remains semantic-only. It must not claim keyboard navigation,
multiple selection, CSS/layout hit testing, default form submission, network
effects, JavaScript handlers, or raw form-value evidence.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-003.md`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

`select` is a supported `combobox` projection and `option` is a supported
semantic role. A single-select with no `selected` option deterministically
selects its first option. A semantic click on an option requires an owning
single-select, selects exactly that option, clears its siblings, and advances
the document revision once. The selected bit is semantic metadata; option
values remain private native state. Failed validation leaves focus, selection,
effects, and revision unchanged.

## Verification

```console
cargo fmt --all -- --check
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
cargo clippy -p glass-browser --features native-engine --all-targets --locked -- -D warnings
cargo test -p glass-browser --all-targets --all-features --locked
```
