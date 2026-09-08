---
id: native-engine-232
scope: glass-browser/native-engine/overflow-inherit
status: planned
depends-on: [native-engine-231]
---

# Native explicit overflow inheritance

## Objective

Extend the bounded overflow family with standalone, case-insensitive
`overflow: inherit`, `overflow-x: inherit`, and `overflow-y: inherit`.
Explicit declarations must copy the parent's effective bounded clip/no-clip
state through the existing private ancestor-style chain while preserving
shorthand/longhand cascade and every existing clip, scroll, paint, hit, and
semantic consumer.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-231.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded `overflow`, `overflow-x`, and `overflow-y` declarations accept
  standalone, case-insensitive `inherit` in addition to their existing
  `hidden|clip|visible` projection and `revert-layer` form. Existing bounded
  `auto|scroll` parsing remains the visible/no-clip projection.
- A winning `overflow: inherit` copies both computed parent axis projections;
  a winning `overflow-x: inherit` or `overflow-y: inherit` copies only its
  corresponding parent axis. At the root, or when a direct unit computation
  has no supplied parent value, each axis uses the existing visible/no-clip
  fallback.
- Omitted overflow declarations remain local and visible/no-clip. Explicit
  finite values, shorthand/longhand order, specificity, `!important`,
  existing `revert-layer` behavior, independent axes, and invalid-later
  preservation remain unchanged. Mixed or token-bearing forms such as
  `inherit hidden`, `hidden inherit`, and `inherit 1px` remain invalid and do
  not replace a preceding valid candidate.
- The resolved axis state continues through existing paint clips, viewport
  projection, root overflow/scroll range, point hit testing, display-list,
  fixed-cell raster/PNG, semantics, and typed diagnostics. No public computed
  style field, dependency, feature default, layout schema, or crate boundary
  is added.

## Boundary and tradeoffs

- Reuse private inherited effective axis projections and existing doubled local
  candidate streams. The engine intentionally copies the effective bounded
  clip/no-clip result rather than preserving `hidden` versus `clip` spelling;
  both already share the same current clip owner.
- This slice covers only explicit inheritance for the three supported overflow
  declarations. Nested scrolling, scrollbars, scroll containers, overflow
  propagation beyond the existing root owner, masks, transforms, additional
  origins, transitions, animations, generic CSS-wide machinery, browser
  parity, and browser-wide overflow conformance remain outside the boundary.
- Omission remains local by contract. Unsupported `auto`/`scroll` behavior
  continues to project to the existing visible/no-clip state rather than
  gaining scrolling semantics.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive shorthand and longhand
  inheritance, supplied parent/root fallback, omitted local fallback,
  independent-axis inheritance, shorthand/longhand precedence, `!important`,
  `revert-layer`, and mixed-invalid forms.
- Run one public fixture through inherited and omitted axis clips, root scroll
  projection, display-list, fixed-cell raster/PNG, point-hit, semantic/source
  order, and typed diagnostics for excluded forms.
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
