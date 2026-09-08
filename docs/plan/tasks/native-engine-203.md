---
id: native-engine-203
scope: glass-browser/native-engine/inherited-text-decoration-line-css-wide-resets
status: planned
depends-on: [native-engine-202]
---

# Native inherited text-decoration-line CSS-wide resets

## Objective

Extend the existing bounded inherited text-decoration line-bit owner with
standalone CSS-wide reset keywords without changing line ownership, decoration
geometry, paint replay, generic cascade machinery, or the two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-202.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded `text-decoration-line` owner, and the existing bounded
  `text-decoration` shorthand route that writes the same line owner, accept
  standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-
  origin `revert`, in addition to finite line sets and `revert-layer`.
- `inherit` and `unset` resolve to the computed parent line-bit set. In the
  current one-author-origin engine, `revert` uses the same parent fallback
  because no lower author origin exists. `revert-layer` remains the only form
  that walks lower named-layer candidates.
- `initial` resolves to the existing finite `none` root fallback. A winning
  reset keyword is terminal for the line owner; invalid later declarations
  preserve the preceding valid declaration, and existing important/source-order
  behavior remains unchanged.
- The public three-bit line representation, immutable text command, display
  list, clipping, capture, line-style/thickness/offset/skip consumers, fixed-cell
  raster, diagnostics, and semantic artifacts remain unchanged.
- Mixed reset tokens, full CSS text-decoration shorthand expansion, additional
  origins, `all`, font metrics, shaping, layout/geometry changes, and browser-
  wide text conformance remain outside this bounded contract.

## Boundary and tradeoffs

- Reuse the existing private declaration/candidate stream and inherited
  resolver. No public computed-style field, display-command field, dependency,
  or crate is introduced.
- Keeping the shorthand route bounded means `text-decoration: inherit` resets
  only the line-bit owner already represented by this engine; it does not imply
  support for every CSS shorthand component or for generic CSS-wide keyword
  machinery.
- The finite `none` initial fallback preserves the current root behavior, while
  inherited and reset values continue through the existing three-bit line
  replay. No new line geometry or raster heuristic is implied.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive standalone forms, root and
  parent fallback, terminal reset behavior, invalid-later preservation,
  `!important`, source order, shorthand/longhand routing, and the
  `revert-layer` distinction.
- Run public fixtures through inherited line combinations, display-list,
  fixed-cell raster/PNG, line-through preservation, and typed diagnostics.
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
