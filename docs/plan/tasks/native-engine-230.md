---
id: native-engine-230
scope: glass-browser/native-engine/text-indent-inherit
status: planned
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

Pending implementation and local certification. Remote CI, push, release, tag,
registry publication, browser-parity, security-boundary certification, and
promotion remain outside this local task.
