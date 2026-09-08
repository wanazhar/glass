---
id: native-engine-204
scope: glass-browser/native-engine/inherited-text-underline-offset-css-wide-resets
status: planned
depends-on: [native-engine-203]
---

# Native inherited text-underline-offset CSS-wide resets

## Objective

Extend the existing bounded inherited `text-underline-offset` owner with
standalone CSS-wide reset keywords without changing underline-only geometry,
shared text commands, fixed-cell raster behavior, generic cascade machinery,
or the two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-203.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded inherited `text-underline-offset` owner accepts standalone,
  case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
  `revert`, in addition to the existing `revert-layer` and signed finite
  `-4px..=4px` values.
- `inherit` and `unset` resolve to the computed parent offset. In the current
  one-author-origin engine, `revert` uses the same parent fallback because no
  lower author origin exists. `revert-layer` remains the only form that walks
  lower named-layer candidates.
- `initial` resolves to the existing finite `0px` root fallback. A winning
  CSS-wide keyword is terminal for this property; invalid later declarations
  preserve the preceding valid declaration, and existing important/source-order
  behavior remains unchanged.
- The reset forms apply through the existing underline-only translation owner.
  Overline and line-through origins, shared text metrics, clipping, capture,
  and fixed-cell raster behavior remain unchanged.
- Mixed reset tokens, `auto`, percentages, fractional or font-derived values,
  dimensions outside the bounded range, additional origins, and generic
  CSS-wide machinery remain outside this bounded contract.

## Boundary and tradeoffs

- Reuse the existing private declaration/candidate stream and inherited
  resolver. No public computed-style field, display-command field,
  dependency, or crate is introduced.
- Keeping the current signed integer range makes reset behavior deterministic
  and preserves the existing replay contract, but it does not claim browser
  support for CSS `auto`, percentages, font-relative values, or arbitrary
  lengths.
- `initial` remains the current engine's zero-pixel fallback rather than
  introducing a new layout or baseline calculation. This keeps the slice
  focused, while browser-derived underline metrics and decoration-origin
  propagation remain future work.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive standalone forms, root and
  parent fallback, terminal reset behavior, invalid-later preservation,
  `!important`, source order, and the `revert-layer` distinction.
- Run one public fixture through inherited signed offsets, underline-only
  movement with overline/line-through preservation, display-list, decoded
  raster/PNG, and typed unsupported-value diagnostics.
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
