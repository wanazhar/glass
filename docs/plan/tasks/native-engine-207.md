---
id: native-engine-207
scope: glass-browser/native-engine/flex-sizing-css-wide-resets
status: planned
depends-on: [native-engine-206]
---

# Native flex sizing CSS-wide resets

## Objective

Extend the existing bounded flex sizing owners with standalone CSS-wide reset
keywords without changing flex placement, shorthand expansion, computed public
schemas, or the two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-206.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded local `flex-grow`, `flex-shrink`, and `flex-basis` owners accept
  standalone, case-insensitive `initial`, `unset`, and one-author-origin
  `revert`, in addition to their existing finite values and `revert-layer`.
- The bounded local `flex` shorthand accepts the same standalone reset forms.
  Each reset expands through the existing private component streams to the
  finite initial tuple `flex-grow:0`, `flex-shrink:1`, and `flex-basis:auto`.
- Reset forms are terminal for the winning local declaration. Invalid later
  declarations preserve the preceding valid declaration; important/source
  order, shorthand/longhand projection, and named-layer `revert-layer` remain
  unchanged.
- Resolved values continue through the existing flex row placement, free-space
  allocation, display-list, fixed-cell raster, point-hit, semantic, and
  diagnostic consumers. No public computed-style field or layout schema is
  added.

## Boundary and tradeoffs

- Reuse the existing private flex component candidate arrays and fallback
  resolvers. A reset-aware route is limited to the flex sizing declarations so
  unrelated direction, wrap, alignment, order, gap, and local CSS parsers do
  not change behavior.
- `inherit`, parent propagation, percentages, negative or fractional values,
  additional origins, transitions, animations, generic CSS-wide machinery,
  flex-basis intrinsic sizing, and browser-wide flex conformance remain outside
  this bounded contract. The existing finite non-negative fixed-pixel basis
  grammar and `auto` value remain the only accepted basis forms.
- `flex-direction`, `flex-wrap`, `flex-flow`, `place-content`, alignment
  declarations, and `order` are intentionally excluded so this slice remains
  a component-expansion/cascade unit rather than a second flex-layout rewrite.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive reset forms, finite initial
  fallbacks, shorthand expansion, terminal reset behavior, invalid-later
  preservation, `!important`, source order, and `revert-layer`.
- Run one public fixture through flex sizing/reset projection, row placement,
  free-space allocation, display-list, fixed-cell raster/PNG, point-hit,
  semantic consumers, and typed diagnostics for excluded values.
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
