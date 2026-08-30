---
id: native-engine-005
scope: glass-browser/native-engine/visibility
status: done
depends-on: [native-engine-004]
---

# Native visibility and actionability gate

## Objective

Implement a bounded presentation-state slice that keeps semantic observation
and interaction aligned:

- recognize `hidden`, `aria-hidden="true"`, and the inline declarations
  `display:none` and `visibility:hidden`;
- omit hidden subtrees from visible-text projection;
- expose a bounded hidden bit in semantic nodes; and
- reject hidden action targets before focus, control state, effects, or revision
  mutation.

This is not a CSS engine. It must not claim selectors, cascade, inheritance,
computed styles, layout, hit testing, opacity behavior, painting, or windowed
rendering.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-004.md`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

Visibility is derived from the node and its ancestors. The supported hidden
signals are the boolean `hidden` attribute, `aria-hidden="true"`, and inline
`style` declarations whose normalized property/value is `display:none` or
`visibility:hidden`. Hidden descendants are excluded from visible text and are
marked `hidden` in the semantic projection. Click/type requests for hidden
targets fail before any mutation. Other style declarations are ignored and
remain outside this slice.

## Verification

```console
cargo fmt --all -- --check
cargo check -p glass-browser --features native-engine --locked
cargo test -p glass-browser --features native-engine --test native_engine --locked
cargo clippy -p glass-browser --features native-engine --all-targets --locked -- -D warnings
cargo test -p glass-browser --all-targets --all-features --locked
```
