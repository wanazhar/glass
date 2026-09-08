---
id: native-engine-233
scope: glass-browser/native-engine/overflow-css-wide-resets
status: complete
depends-on: [native-engine-232]
---

# Native overflow CSS-wide reset semantics

## Objective

Extend the bounded overflow family with standalone, case-insensitive
`initial`, `unset`, and one-author-origin `revert` reset forms for
`overflow`, `overflow-x`, and `overflow-y`. Reset candidates must resolve to
the existing visible/no-clip axis fallback while preserving the explicit
`inherit` parent projection and named-layer `revert-layer` rollback delivered
by the preceding slices.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-232.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded `overflow`, `overflow-x`, and `overflow-y` declarations accept
  standalone, case-insensitive `initial`, `unset`, and one-author-origin
  `revert` in addition to the existing finite projection,
  `inherit`, and `revert-layer` forms.
- Winning reset candidates resolve each affected axis to the existing
  visible/no-clip fallback (`OverflowValue::Other`). `overflow:initial`,
  `overflow:unset`, and `overflow:revert` reset both axes; the x/y longhands
  reset only their corresponding axis.
- Reset candidates do not copy the parent. Explicit `inherit` continues to
  copy the parent's effective clip/no-clip projection, and `revert-layer`
  continues to search lower named-layer candidates before using the local
  fallback. Root and direct-unit fallback behavior remains visible/no-clip.
- Omission remains local and visible/no-clip. Finite values,
  shorthand/longhand order, specificity, `!important`, independent axes,
  mixed-invalid preservation, and invalid-later behavior remain unchanged.
  Token-bearing forms such as `initial hidden`, `unset clip`, and
  `revert 1px` remain invalid and do not replace a preceding valid candidate.
- The resolved axis state continues through existing paint clips, viewport
  projection, root overflow/scroll range, point hit testing, display-list,
  fixed-cell raster/PNG, semantics, and typed diagnostics. No public computed
  style field, dependency, feature default, layout schema, or crate boundary
  is added.

## Boundary and tradeoffs

- Reuse the private doubled x/y candidate streams and the existing local reset
  declaration machinery. Resetting to the current visible/no-clip fallback
  avoids inventing scroll behavior or a second overflow initial-value owner.
- This slice covers only CSS-wide resets for the three bounded overflow
  declarations. Nested scrolling, scrollbars, scroll containers, overflow
  propagation beyond the existing root owner, masks, transforms, additional
  origins, transitions, animations, generic CSS-wide machinery, browser
  parity, and browser-wide overflow conformance remain outside the boundary.
- `revert` remains the bounded one-author-origin fallback; named-layer
  rollback remains the separate `revert-layer` behavior.

## Verification

- Run one native-feature test-target `cargo check` before tests, after the
  implementation batch is complete.
- Focus parser/cascade coverage on case-insensitive shorthand and longhand
  resets, visible/no-clip fallback, independent axes, supplied parent versus
  reset distinction, shorthand/longhand precedence, `!important`,
  `revert-layer` interaction, source order, and mixed-invalid forms.
- Run one public fixture through inherited, finite, reset, and omitted axis
  clips, root scroll projection, display-list, fixed-cell raster/PNG,
  point-hit, semantic/source order, and typed diagnostics for excluded forms.
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

Implementation and local certification are complete at `7b31b72e`; the design
checkpoint is `70f4f924`. The scoped native-feature test-target check passed
before tests. The focused reset-filtered batch passed 1 library-target test and
1 native integration test, covering shorthand and longhand reset fallback,
parent-vs-reset distinction, `!important`, invalid-later preservation,
axis-specific clipping, root scroll projection, display-list, fixed-cell
raster/PNG, point-hit, semantic/source order, and typed diagnostics.
Formatting and diff checks pass. Reset candidates resolve to the existing
visible/no-clip fallback through the private x/y streams; explicit `inherit`
and named-layer `revert-layer` remain distinct, with no public style fields,
dependency, feature default, layout schema, or crate-boundary change. Full
issue-level native integration, feature-library, strict lint, rustdoc, paired
two-crate, package, security/fuzz, static documentation, workspace,
clean-install, remote-CI, issue-sync, and bounded-cleanup gates remain for the
broader issue completion boundary. Remote CI, push, release, tag, registry
publication, browser-parity, security-boundary certification, and promotion
remain outside this local task.
Static documentation checks also pass: release truth reports 647 Markdown
documents (83 current, 59 previous-version hits, 835 semantic audit hits, 0
current-claim failures); coverage reports 647 Markdown files, 345 full-product
MCP tools (100 browser-only), 17 examples, and 22 public modules; depth reports
93 current guides and 19 substantive contracts; feature parity reports 14
capabilities across 4 targets; TUI reports 15 implementation help keys and 63
documentation markers; and version sync reports `0.3.14`.
