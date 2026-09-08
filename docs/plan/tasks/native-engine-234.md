---
id: native-engine-234
scope: glass-browser/native-engine/overflow-no-clip-keywords
status: planned
depends-on: [native-engine-233]
---

# Native overflow no-clip keywords

## Objective

Complete the bounded finite overflow keyword set by accepting standalone,
case-insensitive `visible`, `auto`, and `scroll` for `overflow`, `overflow-x`,
and `overflow-y`. These values must project to the existing visible/no-clip
`OverflowValue::Other` state without adding nested scrolling, scrollbars, or a
second scroll-container owner.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-233.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- Standalone, case-insensitive `visible`, `auto`, and `scroll` are supported
  finite values for the overflow shorthand and both physical longhands.
  All three resolve to the existing visible/no-clip `OverflowValue::Other`
  projection.
- The values do not create nested scroll containers, scrollbar artifacts, or
  new scroll ownership. Existing root viewport scrolling continues to derive
  from visible content extent, and the current hidden/clip axis clips remain
  unchanged.
- Explicit `inherit` continues to copy the parent's effective clip/no-clip
  projection. `initial`, `unset`, and bounded one-author-origin `revert`
  continue to reset the affected axis to visible/no-clip, while
  `revert-layer` continues lower-layer rollback. Important/source-order,
  shorthand/longhand precedence, independent axes, and invalid-later
  preservation remain unchanged.
- Token-bearing or mixed forms such as `visible hidden`, `auto clip`, and
  `scroll 1px` remain invalid and produce the existing typed unsupported-value
  diagnostic. Standalone supported keywords do not produce an unsupported
  overflow diagnostic.
- The resolved state continues through existing root overflow/scroll range,
  paint, display-list, fixed-cell raster/PNG, point-hit, semantics, and
  diagnostics. No public computed style field, dependency, feature default,
  layout schema, or crate boundary is added.

## Boundary and tradeoffs

- Reuse the existing `OverflowValue::Other` projection rather than adding a
  public keyword enum or a new layout owner. This keeps the implementation
  honest about the current absence of nested scrolling and scrollbar behavior.
- This slice covers only finite no-clip keyword acceptance and diagnostics.
  Nested scrolling, scrollbars, scroll containers, overflow propagation beyond
  the existing root owner, masks, transforms, additional origins, transitions,
  animations, generic CSS-wide machinery, browser parity, and browser-wide
  overflow conformance remain outside the boundary.
- The native engine may therefore render `auto` and `scroll` without scroll
  container behavior; that is an explicit experimental boundary, not a claim
  of browser used-value parity.

## Verification

- Run one native-feature test-target `cargo check` before tests, after the
  implementation batch is complete.
- Focus parser/cascade coverage on case-insensitive shorthand and longhand
  keywords, no-clip fallback, independent axes, parent/reset interaction,
  `!important`, `revert-layer`, source order, invalid mixed forms, and typed
  diagnostics.
- Run one public fixture through finite no-clip keywords beside hidden/clip,
  root scroll projection, display-list, fixed-cell raster/PNG, point-hit,
  semantic/source order, and diagnostic absence/presence.
- Near issue completion retain the full native integration, feature-library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, clean-install, issue-sync,
  and bounded cleanup gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Cleanup

Only exact task-specific regenerable targets and reports may be removed after
Cargo/Rust process and open-handle checks. Source, durable data, repository
targets, issue snapshots, and unrelated workloads remain untouched.

## Completion evidence

Pending implementation and local certification. Remote CI, push, release, tag,
registry publication, browser-parity, security-boundary certification, and
promotion remain outside this local task.
