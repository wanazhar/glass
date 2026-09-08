---
id: native-engine-213
scope: glass-browser/native-engine/local-align-content-css-wide-resets
status: planned
depends-on: [native-engine-212]
---

# Native align-content CSS-wide resets

## Objective

Extend the existing bounded local `align-content` owner with standalone
CSS-wide reset keywords without changing wrapped-line formation, item-level
alignment, `place-content` shorthand projection, or the two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-212.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded non-inherited `align-content` owner accepts standalone,
  case-insensitive `initial`, `unset`, and one-author-origin `revert`, in
  addition to its existing finite wrapped-line distribution values and
  `revert-layer`.
- Each reset form resolves through the existing local resolver to its bounded
  `FlexStart` fallback. A winning reset is terminal for the local declaration;
  invalid later declarations preserve the preceding valid declaration, while
  important/source order and named-layer `revert-layer` behavior remain
  unchanged.
- The resolved value continues through existing wrapped-line distribution,
  `align-items`/`align-self` placement, flex sizing, display-list, fixed-cell
  raster, PNG, point-hit, semantic, and diagnostic consumers. No public
  computed-style field or layout schema is added.

## Boundary and tradeoffs

- Reuse the existing private `align-content` candidate array and resolver. Only
  this local parser and fallback path changes; `place-content` shorthand,
  item-level alignment, inherited `direction`, and unrelated CSS parsers
  remain stable.
- Keeping `inherit`, percentages, baseline/safe/unsafe forms, additional
  origins, transitions, animations, generic CSS-wide machinery, and
  browser-wide alignment conformance outside the slice avoids implying parent
  propagation or a general cascade rewrite.
- The reset forms intentionally reuse the engine's current bounded
  `FlexStart` fallback. The slice does not invent a new line-distribution model
  or silently broaden `place-content` shorthand reset behavior.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive reset forms, the bounded
  `FlexStart` fallback, terminal reset behavior, invalid-later preservation,
  `!important`, source order, finite values, and `revert-layer`.
- Run one public fixture through wrapped-line distribution, item alignment,
  flex sizing, display-list, fixed-cell raster/PNG, point-hit/semantic
  consumers, and typed unsupported-value diagnostics for excluded forms.
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
