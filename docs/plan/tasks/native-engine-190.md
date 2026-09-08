---
id: native-engine-190
scope: glass-browser/native-engine/cascade-dimension-important-priority
status: planned
depends-on: [native-engine-189]
---

# Native dimension `!important` cascade priority

## Objective

Extend the bounded author-origin `!important` cascade contract to the six
normal-only local dimension declarations already consumed by the native
box/layout owner:

- `width` and `height`;
- `min-width` and `max-width`; and
- `min-height` and `max-height`.

## Contract

- Each listed declaration accepts the existing bounded non-negative integer
  pixel grammar or a standalone case-insensitive `revert-layer`, followed by
  a terminal case-insensitive `!important` marker.
- Important candidates outrank normal candidates. Within the bounded
  author-important partition, earliest named layers win, later named layers
  follow, unlayered important candidates are lowest, and inline important
  candidates retain inline precedence in the unlayered important bucket.
- The six dimensions keep independent candidate streams and existing
  `None` fallbacks. `revert-layer !important` rolls back only the current
  important layer before lower candidates are considered; it does not erase
  normal candidates or values for another dimension.
- Valid declaration values and their importance survive invalid later
  declarations. Existing computed optional dimensions, min/max constraint
  ordering, content-box/border-box geometry, normal flow, flex placement,
  overflow projection, display-list, raster, capture, point-hit, and
  semantic/source-order consumers remain unchanged.

## Boundary and tradeoffs

- Only the six listed dimension declarations receive this priority behavior.
  Box sizing, padding, margin, overflow, and remaining normal-only properties
  stay outside generic `!important` semantics until separately contracted.
- Six private doubled candidate arrays and six bounded importance bits add
  fixed per-style storage. No importance provenance enters computed values,
  layout boxes, display-list commands, raster surfaces, captures, hit targets,
  public schemas, or diagnostics.
- The slice retains the existing author-origin, finite non-negative integer
  pixel, horizontal-tb, local fixture/data-URL, and two-crate boundaries.
  Percentages, negative/auto/intrinsic dimensions, aspect ratio, multiple
  origins, transitions, animations, browser sizing parity, dependencies,
  default features, and crate boundaries remain unchanged.
- Reusing the existing `LocalCascadeDeclaration<u32>` and local resolver keeps
  the implementation small and preserves `revert-layer` behavior. It also
  deliberately does not broaden parsing or introduce a general CSS property
  registry; unsupported dimension values remain ignored as they are today.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus unit coverage on terminal marker parsing for all six properties,
  invalid-later preservation, important-over-normal priority, reversed named
  layers, unlayered/inline important precedence, independent dimension
  streams, and `revert-layer !important` rollback.
- Run one native integration fixture through min/max constraint interaction,
  box geometry, normal-flow/flex consumers, overflow/scroll projection,
  display-list/raster/capture, point-hit, and semantic/source-order outputs.
- Near issue completion retain the full native integration, feature library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, and bounded cleanup gates.

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

To be filled after the implementation, documentation, local certification,
and exact cleanup pass. Issue-level final gates, remote CI, push, release,
tag, registry publication, browser parity, security-boundary certification,
and promotion remain deferred until the epic's final validation boundary.
