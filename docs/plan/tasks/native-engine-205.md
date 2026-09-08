---
id: native-engine-205
scope: glass-browser/native-engine/local-gap-css-wide-resets
status: planned
depends-on: [native-engine-204]
---

# Native local gap CSS-wide resets

## Objective

Extend the existing bounded local `gap`, `row-gap`, and `column-gap` owners
with standalone CSS-wide reset keywords without changing flex/grid placement,
shorthand/longhand source order, named-layer rollback, display artifacts, or
the two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-204.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded local `gap`, `row-gap`, and `column-gap` declarations accept
  standalone, case-insensitive `initial`, `unset`, and one-author-origin
  `revert`, in addition to the existing finite non-negative pixel values and
  `revert-layer`.
- `initial`, `unset`, and `revert` resolve to the current local zero-gap
  fallback. A winning reset candidate is terminal for that axis; invalid later
  declarations preserve the preceding valid declaration, and existing
  shorthand/longhand, important, source-order, and named-layer behavior remain
  unchanged.
- `gap` reset forms write both existing row and column candidate streams;
  `row-gap` and `column-gap` reset forms affect only their respective axis.
  The resolved axes continue through the existing flex placement, normal-flow,
  display-list, raster, hit, and semantic consumers.
- `inherit`, percentages, negative/fractional lengths, intrinsic values,
  grid-specific track sizing, additional origins, and generic CSS-wide
  machinery remain outside this local bounded contract. In particular, this
  slice does not add parent-gap propagation to the inherited-style carrier.

## Boundary and tradeoffs

- Reuse the private gap shorthand/longhand candidate arrays and existing
  zero-value resolver. No public computed-style field, display-command field,
  dependency, or crate is introduced.
- Keeping `inherit` unsupported avoids changing the DOM-to-style inheritance
  contract for a non-inherited layout property; callers receive the existing
  typed unsupported-value diagnostic rather than a misleading parent value.
- Resetting to zero preserves the current local omission fallback and keeps
  flex placement deterministic, but does not imply browser-wide grid or gap
  conformance.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive reset forms, zero fallback,
  terminal reset behavior, invalid-later preservation, shorthand/longhand
  axis projection, `!important`, source order, and `revert-layer`.
- Run one public fixture through row/column placement, display-list, fixed-cell
  raster/PNG, point-hit/semantic consumers, and typed unsupported-value
  diagnostics for excluded `inherit`/unsupported lengths.
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
