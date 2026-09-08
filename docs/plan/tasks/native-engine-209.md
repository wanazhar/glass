---
id: native-engine-209
scope: glass-browser/native-engine/local-flex-order-css-wide-resets
status: planned
depends-on: [native-engine-208]
---

# Native flex-item order CSS-wide resets

## Objective

Extend the existing bounded local flex-item `order` owner with standalone
CSS-wide reset keywords without changing visual sorting, source-order ties, or
the two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-208.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded non-inherited `order` owner accepts standalone,
  case-insensitive `initial`, `unset`, and one-author-origin `revert`, in
  addition to its existing signed integer range `-1024..=1024` and
  `revert-layer`.
- Each reset form resolves through the existing local order resolver to the
  finite `0` initial fallback. A winning reset is terminal for the local
  declaration; invalid later declarations preserve the preceding valid
  declaration, while important/source order and named-layer `revert-layer`
  behavior remain unchanged.
- The resolved order continues through existing stable visual sorting, source
  order ties, flex sizing/line formation, display-list, fixed-cell raster,
  point-hit, semantic, and diagnostic consumers. No public computed-style
  field or layout schema is added.

## Boundary and tradeoffs

- Reuse the existing private `order` candidate array and resolver. Only the
  local item-order parser and fallback path changes; flex direction, wrapping,
  alignment, inherited `direction`, and unrelated CSS parsers remain stable.
- Keeping `inherit`, percentages, fractional values, additional origins,
  transitions, animations, generic CSS-wide machinery, and browser-wide
  ordering conformance outside the slice avoids implying parent propagation or
  a general cascade rewrite.
- Resetting to zero preserves the current omitted-value fallback and makes
  `initial`, `unset`, and local `revert` deterministic, while finite signed
  ordering and stable source-order ties remain the only supported placement
  behavior.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive reset forms, finite zero
  fallback, terminal reset behavior, invalid-later preservation, `!important`,
  source order, signed finite values, and `revert-layer`.
- Run one public fixture through visual order, source-order ties, flex sizing,
  display-list, fixed-cell raster/PNG, point-hit/semantic consumers, and typed
  unsupported-value diagnostics for excluded `inherit`/unsupported values.
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
