---
id: native-engine-201
scope: glass-browser/native-engine/inherited-text-decoration-thickness-css-wide-resets
status: planned
depends-on: [native-engine-200]
---

# Native inherited text-decoration-thickness CSS-wide resets

## Objective

Extend the existing bounded inherited `text-decoration-thickness` owner with
standalone CSS-wide reset keywords without changing decoration geometry,
fixed-cell raster behavior, generic cascade machinery, or the two-crate
boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-200.md`

## Contract

- Inherited `text-decoration-thickness` accepts standalone, case-insensitive
  `inherit`, `initial`, `unset`, and one-author-origin `revert`, in addition to
  the existing `revert-layer` and finite positive `1px|2px|3px|4px` values.
- `inherit` and `unset` resolve to the computed parent thickness. In the
  current one-author-origin engine, `revert` uses the same parent fallback
  because no lower author origin exists. `revert-layer` remains the only form
  that walks lower named-layer candidates.
- `initial` resolves to the existing finite `1px` root fallback. A winning
  CSS-wide keyword is terminal for this property; invalid later declarations
  preserve the preceding valid declaration, and existing important/source-order
  behavior remains unchanged.
- Mixed reset tokens, `auto`, `from-font`, percentages, fractional or
  out-of-range lengths, additional origins, and generic CSS-wide machinery
  remain outside this bounded contract.

## Boundary and tradeoffs

- Reuse the existing private declaration/candidate stream and inherited
  resolver. No new public computed-style field, artifact value, dependency, or
  crate is introduced.
- The resolved finite thickness continues through the existing shared text
  command, decoration-style pattern, display-list, capture, and fixed-cell
  raster owners. This slice does not change band placement, dash/dot periods,
  style, offset, skip behavior, color, line propagation, or layout.
- The explicit `1px` initial fallback preserves the current root behavior;
  browser `auto`/`from-font` used-value computation is deliberately not
  implied by accepting CSS-wide reset keywords.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive standalone forms, root and
  parent fallback, terminal reset behavior, invalid-later preservation,
  `!important`, source order, and the `revert-layer` distinction.
- Run one public fixture through inherited thickness, shared style/line
  geometry, display-list, decoded raster/PNG, and diagnostic consumers.
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

To be filled after implementation and local certification. Remote CI, push,
release, tag, registry publication, browser parity, security-boundary
certification, and promotion remain outside this local task.
