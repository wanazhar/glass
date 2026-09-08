---
id: native-engine-229
scope: glass-browser/native-engine/gap-longhand-inherit
status: planned
depends-on: [native-engine-228]
---

# Native explicit `row-gap: inherit` and `column-gap: inherit`

## Objective

Complete the bounded CSS-wide inheritance path for the direct gap longhands.
Standalone, case-insensitive `row-gap: inherit` and `column-gap: inherit` must
copy their corresponding computed parent pixel components through the existing
private ancestor-style chain while preserving the completed `gap: inherit`
shorthand owner and all existing layout/artifact consumers.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-228.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded `row-gap` and `column-gap` longhands accept standalone,
  case-insensitive `inherit` in addition to their existing finite pixel and
  `initial|unset|revert|revert-layer` forms.
- A winning `row-gap: inherit` resolves the computed parent row gap; a winning
  `column-gap: inherit` resolves the computed parent column gap. At the root,
  or when a direct unit computation has no supplied parent value, each axis
  uses the existing bounded `0` fallback.
- Omitted longhands remain local and resolve to `0`; explicit finite values,
  shorthand/longhand declaration order, specificity, important priority,
  reset semantics, and `revert-layer` rollback remain unchanged. Mixed or
  token-bearing forms such as `inherit 1px`, `1px inherit`, or a longhand
  declaration with both axes remain invalid and do not replace a preceding
  valid candidate.
- The completed `gap: inherit` shorthand continues to copy both computed
  parent components. Resolved longhands continue through wrapped/column flex
  placement, display-list, fixed-cell raster, PNG, point-hit,
  semantic/source-order, and typed-diagnostic paths. No public computed-style
  field, layout schema, dependency, feature default, or crate boundary is
  added.

## Boundary and tradeoffs

- Reuse the private row/column inherited values and existing `GapCascade`
  component streams. Do not create a public inheritance API, duplicate layout
  state, or change semantic/source traversal.
- This slice covers only direct gap longhand inheritance. Percentages,
  intrinsic values, additional origins, transitions, animations, grid track
  sizing, generic CSS-wide machinery, browser parity, and browser-wide gap
  conformance remain outside the boundary.
- Explicit inheritance copies computed pixel components rather than source
  declarations. The root `0` fallback is deterministic but is not a claim of
  browser-wide CSS conformance.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive standalone longhand
  inheritance, supplied parent row/column values, root/default fallback,
  omitted local fallback, finite values, mixed-invalid forms, `!important`,
  shorthand/longhand precedence, reset handling, and `revert-layer`.
- Run one public fixture through independent row/column inherited spacing,
  wrapped and column placement, shorthand/longhand override order, complete
  artifact movement, display-list, fixed-cell raster/PNG, point-hit,
  semantic/source-order preservation, and typed diagnostics for excluded forms.
- Near issue completion retain the full native integration, feature-library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, clean-install, issue-sync,
  and bounded cleanup gates.

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
