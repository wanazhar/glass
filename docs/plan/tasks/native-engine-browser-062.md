id: native-engine-browser-062
scope: glass-browser/native-engine/form-data-select-controls
status: done
depends-on: [native-engine-browser-061]
---

# BE-03ag/BE-04ar: bounded FormData select controls

## Objective

Complete the useful text-only `FormData(form)` control set for textarea,
single-select, and initially selected multi-select controls without creating a
second form-serialization owner.

## Contract

- `textarea` values use the existing Rust-owned current value and preserve
  document order.
- Single-select controls contribute their selected option value, falling back
  to option text when no `value` attribute exists.
- Multi-select controls contribute every initially selected, enabled option in
  option order; empty selected values remain absent rather than inventing a
  default option.
- Local and process-backed snapshots use the same `parentIndex`, selected
  state, disabled state, and attribute metadata; existing file-control
  rejection and fetch/XHR serialization remain unchanged.

## Deliberate boundary and tradeoffs

This slice does not add interactive multi-select pointer/keyboard behavior,
`optgroup` disabled-state inheritance, `File`/`Blob` parts, the `formdata`
event, or full live FormData iterator/Web IDL identity. The parser now
preserves multiple selected options instead of collapsing them, while the
existing interactive action path continues to reject multiple-select actions
explicitly until that separate behavior is implemented.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine form_data_constructor -- --nocapture` — 2 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.
