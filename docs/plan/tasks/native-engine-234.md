---
id: native-engine-234
scope: glass-browser/native-engine/overflow-no-clip-keywords
status: complete
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

Implementation and local certification are complete at `8a96f56b`; the design
checkpoint is `b2119e5f`. The scoped native-feature test-target check passed
before tests. The focused no-clip keyword batch passed 1 library-target test
and 1 native integration test, and the broader overflow-filtered regression
batch passed 11 library-target tests and 21 matching native integration tests.
The evidence covers case-insensitive `visible|auto|scroll`, independent
longhands, no-clip/root-scroll projection, display-list, fixed-cell raster/PNG,
point-hit, semantic/source order, diagnostic absence for supported values, and
diagnostic presence for mixed invalid forms. Formatting and diff checks pass.
The implementation reuses `OverflowValue::Other`; it adds no nested scroll
container, scrollbar, public style field, dependency, feature default, layout
schema, or crate-boundary behavior. Full issue-level native integration,
feature-library, strict lint, rustdoc, paired two-crate, package,
security/fuzz, static documentation, workspace, clean-install, remote-CI,
issue-sync, and bounded-cleanup gates remain for the broader issue completion
boundary. Remote CI, push, release, tag, registry publication, browser-parity,
security-boundary certification, and promotion remain outside this local task.
Static documentation checks also pass: release truth reports 648 Markdown
documents (83 current, 59 previous-version hits, 838 semantic audit hits, 0
current-claim failures); coverage reports 648 Markdown files, 345 full-product
MCP tools (100 browser-only), 17 examples, and 22 public modules; depth reports
93 current guides and 19 substantive contracts; feature parity reports 14
capabilities across 4 targets; TUI reports 15 implementation help keys and 63
documentation markers; and version sync reports `0.3.14`.
