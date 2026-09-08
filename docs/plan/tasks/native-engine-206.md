---
id: native-engine-206
scope: glass-browser/native-engine/local-text-indent-overflow-css-wide-resets
status: planned
depends-on: [native-engine-205]
---

# Native local text-indent and text-overflow CSS-wide resets

## Objective

Extend the existing bounded local `text-indent` and `text-overflow` owners
with standalone CSS-wide reset keywords without changing first-line geometry,
overflow state, display artifacts, or the two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-205.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded local `text-indent` owner accepts standalone, case-insensitive
  `initial`, `unset`, and one-author-origin `revert`, in addition to its finite
  non-negative pixel values and `revert-layer`.
- The bounded local `text-overflow` owner accepts standalone, case-insensitive
  `initial`, `unset`, and one-author-origin `revert`, in addition to its finite
  `clip|ellipsis` values and `revert-layer`.
- All three reset forms are terminal for the winning local declaration:
  `text-indent` resolves to the existing `0px` fallback and `text-overflow`
  resolves to the existing `clip` fallback. Invalid later declarations
  preserve the preceding valid declaration; important/source-order and
  named-layer rollback remain unchanged.
- `text-indent` continues through first-line layout, display-list, fixed-cell
  raster, point-hit, and semantic consumers. `text-overflow` continues through
  its existing computed-style owner without introducing ellipsis layout or
  glyph truncation.
- `inherit`, parent propagation, percentages, negative/fractional lengths,
  additional origins, generic CSS-wide machinery, and browser-wide overflow
  conformance remain outside this local bounded contract.

## Boundary and tradeoffs

- Add a private reset-aware parser route for these two declarations rather than
  widening every local declaration parser. Existing border, background,
  display, opacity, visibility, dimension, padding, margin, and box-sizing
  contracts remain unchanged.
- Reuse the existing `LocalCascadeDeclaration::Reset` fallback path. No public
  computed-style field, display-command field, dependency, or crate is
  introduced.
- Keeping `inherit` unsupported avoids implying parent propagation for these
  local owners; callers receive the existing typed unsupported-value diagnostic.
  The native engine does not claim browser-wide text-overflow truncation.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive reset forms, finite local
  fallbacks, terminal reset behavior, invalid-later preservation,
  `!important`, source order, and `revert-layer`.
- Run one public fixture through text-indent layout, display-list, fixed-cell
  raster/PNG, point-hit/semantic consumers, and typed unsupported-value
  diagnostics for excluded `inherit`/unsupported values.
- Near issue completion retain the full native integration, feature-library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, clean-install, issue-sync,
  and bounded cleanup gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Cleanup

Only exact task-specific regenerable targets and reports may be removed after
Cargo/Rust process and open-handle checks. Source, durable data, repository
targets, issue snapshots, and unrelated workloads remain untouched.

## Completion evidence

Implementation and validation evidence will be recorded here after the
bounded implementation and documentation closeout. Remote CI, push, release,
tag, registry publication, browser-parity, security-boundary certification,
and promotion remain outside this local task.
