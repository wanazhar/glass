---
id: native-engine-220
scope: glass-browser/native-engine/local-flex-direction-inherit
status: planned
depends-on: [native-engine-219]
---

# Native explicit `flex-direction: inherit`

## Objective

Add one bounded explicit inheritance path for the local `flex-direction`
owner. A standalone `inherit` must copy the already computed parent direction
through the existing private ancestor-style chain while preserving the local
`row` fallback, shorthand/component cascade, and all current row/column layout
consumers.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-208.md`
- `docs/plan/tasks/native-engine-219.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded local `flex-direction` owner accepts standalone,
  case-insensitive `inherit` in addition to its existing finite
  `row|row-reverse|column|column-reverse` values, CSS-wide reset forms, and
  `revert-layer`.
- A winning `flex-direction: inherit` resolves to the computed
  `flex-direction` value supplied by the parent-style chain. At the root, or
  when a direct unit computation has no supplied parent value, the existing
  bounded `row` fallback remains the result.
- Omitted `flex-direction` remains local and resolves to `row`; finite values
  remain explicit local owners. Mixed forms such as `inherit column` remain
  invalid and do not replace a preceding valid candidate. Existing
  `flex-flow` shorthand projection, same-block longhand precedence,
  important/source order, reset semantics, `revert-layer` rollback, and
  `flex-wrap` ownership remain unchanged.
- The inherited direction continues through row/row-reverse and
  column/column-reverse main-axis mapping, wrapping eligibility, gap and
  margin mapping, flex sizing, complete-subtree movement, display-list,
  fixed-cell raster, PNG, point-hit, semantic/source order, and diagnostics.
  No public computed-style field, layout schema, dependency, feature default,
  or crate boundary is added.

## Boundary and tradeoffs

- Add only the `flex-direction` value to `NativeInheritedStyle` and reuse the
  existing ancestor walk. `flex-wrap` remains a separate local owner, so this
  slice does not make `flex-flow` or the entire flex shorthand inherited.
- Explicit `inherit` copies a computed value, not declarations, and does not
  make finite directions implicitly inherited. The bounded root fallback is
  deterministic but is not a claim of browser-wide CSS conformance.
- `flex-wrap:inherit`, `flex-flow:inherit`, percentages, intrinsic sizing,
  additional origins, transitions, animations, generic CSS-wide machinery,
  writing-mode, grid, browser parity, security boundaries, and remote CI
  remain outside this slice.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on standalone case-insensitive `inherit`, all
  finite direction values, supplied-parent and root/default fallback, omitted
  local fallback, invalid mixed/later forms, `!important`, source order,
  `flex-flow` component precedence, and `revert-layer` preservation.
- Run one public nested-flex fixture through parent-to-child direction
  propagation, row/column placement, wrapping eligibility, gap/margin mapping,
  flex sizing, complete-subtree movement, display-list, fixed-cell raster/PNG,
  point-hit, semantic/source order, and typed diagnostics for excluded mixed
  forms.
- Near issue completion retain the full native integration, feature-library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, clean-install, issue-sync,
  and bounded cleanup gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Cleanup

Only exact task-specific regenerable targets and reports may be removed after
Cargo/Rust process and open-handle checks. Source, durable data, repository
targets, issue snapshots, and unrelated workloads remain untouched.

## Completion evidence

Pending implementation and local certification. Remote CI, push, release,
tag, registry publication, browser-parity, security-boundary certification,
and promotion remain outside this local task.
