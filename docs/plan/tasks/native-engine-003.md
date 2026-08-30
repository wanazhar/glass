---
id: native-engine-003
scope: glass-browser/native-engine/interaction
status: done
depends-on: [native-engine-002]
---

# Native revisioned semantic interaction

## Objective

Implement the first real Phase 2 mutation path:

- accept semantic click and type actions for supported local controls;
- maintain focus, text-control state, checkbox/radio state, and one monotonic
  revision owner;
- record bounded native event/effect metadata;
- expose action and effects through the existing native backend dispatcher; and
- reject stale, detached, ambiguous, disabled, read-only, and unsupported
  targets before mutation.

The action capability is semantic-only. This task must not claim CSS/layout hit
testing, coordinate input, default link navigation, JavaScript handlers, or raw
form-value transport evidence.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-002.md`
- `crates/glass-browser/src/browser_backend.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`

## Path

- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/error.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/backend-capability-matrix.json`
- `docs/experimental-capabilities.md`
- `docs/browser-host-rfc.md`
- `docs/features.md`
- `README.md`
- `crates/glass-browser/README.md`
- `docs/plan/README.md`

## Contract

`SemanticAction::Click` and `SemanticAction::Type` are dispatched only after
the native locator resolves against the current revision. Each accepted
action advances the revision exactly once. Effects return the current revision
and a changed bit; native event details remain internal and bounded. KeyPress,
Scroll, CSS/layout hit testing, network, script, and raw values remain
unavailable.

## Verification

```console
cargo fmt --all -- --check
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
cargo clippy -p glass-browser --features native-engine --all-targets --locked -- -D warnings
cargo test -p glass-browser --all-targets --all-features --locked
```
