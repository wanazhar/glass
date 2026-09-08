---
id: native-engine-230
scope: glass-browser/native-engine/text-indent-inherit
status: complete
depends-on: [native-engine-229]
---

# Native explicit `text-indent: inherit`

## Objective

Extend the bounded first-line text geometry owner with standalone,
case-insensitive `text-indent: inherit`. An explicitly authored declaration
must copy the computed parent non-negative pixel indent through the existing
private ancestor-style chain while preserving the current local fallback and
all first-line layout and artifact consumers.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-229.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded `text-indent` declaration accepts standalone,
  case-insensitive `inherit` in addition to its existing non-negative
  fixed-pixel and `initial|unset|revert|revert-layer` forms.
- A winning `text-indent: inherit` resolves the computed parent first-line
  indent. At the root, or when a direct unit computation has no supplied
  parent value, it uses the existing bounded `0` pixel fallback.
- Omitted `text-indent` remains local and resolves to `0`; explicit finite
  values, source order, specificity, `!important`, reset semantics,
  `revert-layer`, and invalid-later preservation remain unchanged. Mixed or
  token-bearing forms such as `inherit 1px` remain invalid and do not replace
  a preceding valid candidate.
- The resolved value continues through block first-line placement, wrapping,
  text fragments, display-list, fixed-cell raster/PNG, overflow/scroll,
  point-hit, and semantic/source-order consumers. No public computed-style
  field, dependency, feature default, layout schema, or crate boundary is
  added.

## Boundary and tradeoffs

- Reuse the private inherited pixel state and the existing local candidate
  stream. Do not create a public inheritance API, duplicate first-line
  geometry, or change inline-item behavior.
- This slice covers only explicit `text-indent` inheritance. Negative or
  hanging indentation, percentages, font-relative units, marker styling,
  multiple origins, transitions, animations, generic CSS-wide machinery,
  `text-overflow`, and browser-wide text conformance remain outside the
  boundary.
- Explicit inheritance copies the computed pixel value rather than source
  declarations. Omission remains local by contract even though broader CSS
  inheritance behavior is outside this bounded native engine.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive standalone inheritance,
  supplied parent/root fallback, omitted local fallback, finite values,
  mixed-invalid forms, `!important`, source order, shorthand-independent
  reset behavior, and `revert-layer`.
- Run one public fixture through nested first-line placement and wrapping,
  display-list, fixed-cell raster/PNG, overflow/scroll, point-hit,
  semantic/source-order, and typed diagnostics for excluded forms.
- Near issue completion retain the full native integration, feature-library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz,
  static-documentation, workspace all-target/all-feature, clean-install,
  issue-sync, and bounded cleanup gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Cleanup

Only exact task-specific regenerable targets and reports may be removed after
Cargo/Rust process and open-handle checks. Source, durable data, repository
targets, issue snapshots, and unrelated workloads remain untouched.

## Completion evidence

Implementation and local certification are complete at `5d214fee` (design
`3782eb4b`). The focused parser/cascade group passes 3/3 tests, including
case-insensitive standalone inheritance, omitted-property non-inheritance,
mixed-invalid preservation, source order, important priority, and reset
fallbacks. The public inheritance fixture passes 1/1 and verifies first-line
layout coordinates, display-list text projection, fixed-cell raster/PNG,
point hit testing, semantic/source order, and the typed diagnostic for the
excluded mixed form. Full native integration passes 268/268, and the
feature-enabled `glass-browser` library passes 1,047 tests with 1 ignored.
Static documentation checks also pass: release truth reports 644 Markdown
documents (83 current, 59 previous-version hits, 825 semantic audit hits, 0
current-claim failures); coverage reports 644 Markdown files, 345 full-product
MCP tools (100 browser-only), 17 examples, and 22 public modules; depth reports
93 current guides and 19 substantive contracts; feature parity reports 14
capabilities across 4 targets; TUI reports 15 implementation help keys and 63
documentation markers; and version sync reports `0.3.14`. Formatting and diff
checks pass. Issue-level strict lint, rustdoc, paired
two-crate, package, security/fuzz, static-documentation, workspace,
clean-install, remote-CI, issue-sync, and bounded-cleanup gates remain for
the broader issue completion boundary; remote CI, push, release, tag,
registry publication, browser-parity, security-boundary certification, and
promotion remain outside this local task.
