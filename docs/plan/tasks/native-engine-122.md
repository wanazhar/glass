---
id: native-engine-122
scope: glass-browser/native-engine/cascade-layers-revert-layer
status: planned
depends-on: [native-engine-121]
---

# Native bounded cascade layers and `revert-layer`

## Objective

Add a bounded, deterministic CSS cascade-layer model to the existing native
stylesheet owner and support the explicit `revert-layer` keyword for the
inherited `text-decoration-skip-spaces` property. This advances the cascade
boundary without widening the public paint value or introducing a second style
system.

## Context

The 119, 120, and 121 slices added declaration-only handling for
`inherit`, `unset`, and `revert` while keeping the resolved
`NativeTextDecorationSkipSpaces` enum finite. `revert-layer` needs a real layer
ordering and rollback boundary; treating it as another spelling of `revert`
would erase the distinction this slice is meant to establish.

The normative property reference is:

- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-skip-spaces-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-121.md`

## Contract

### Layer collection and priority

- The parser accepts only top-level named blocks of the form
  `@layer <identifier> { ... }`.
- A named layer's rank is assigned by first appearance across the stylesheet
  source list. Reopening the same name reuses its rank and contributes normal
  source order within that layer.
- The implementation is bounded to 15 named layers. The implicit unlayered
  author bucket is always above those named layers for ordinary declarations.
- A rule inside a named layer participates in the existing selector matching,
  specificity, source-order, and inline-style paths. Layer priority is inserted
  before selector specificity, so a later named layer wins over an earlier one
  even when the earlier selector is more specific. Within one layer, the
  existing precedence rules remain unchanged.
- The layer rank is private cascade metadata. It must not enter
  `NativeDisplayCommand`, layout geometry, raster data, semantic output, or any
  public API.
- Inline declarations remain in the unlayered bucket and therefore outrank
  ordinary named-layer declarations through the existing inline precedence.
- `@layer` statements, anonymous layers, comma-separated layer names, nested
  layer blocks, and other at-rules remain unsupported typed diagnostics. A
  malformed or over-limit layer must not leak raw stylesheet content.

### `revert-layer`

- `text-decoration-skip-spaces: revert-layer` is accepted as one
  case-insensitive token and stored only as a private declaration state.
- The winning candidate is resolved by removing the candidate layer and then
  selecting the highest-priority remaining declaration for the same property.
  A named-layer declaration rolls back to the next lower layer; an unlayered
  declaration rolls back to the highest named layer.
- Repeated `revert-layer` declarations in descending layers continue the same
  bounded rollback until a concrete value or a CSS-wide fallback is found. If
  no lower declaration exists, the property uses its existing inherited
  computed fallback; root fallback remains `None`.
- `revert` continues to use the current one-author-origin inherited fallback;
  `inherit`, `unset`, and `initial` retain their existing meanings.
- `revert-layer` is intentionally supported only for
  `text-decoration-skip-spaces` in this slice. The token remains an explicit
  unsupported-value diagnostic for other supported properties.
- The public resolved value remains the finite
  `None|All|Start|End|StartAndEnd` paint value. No unresolved keyword can reach
  `NativeDisplayCommand` or the software rasterizer.

### Existing owners preserved

The resolved value continues through the existing inherited computed style,
immutable text command, line-edge provenance, Unicode whitespace classifier,
and software decoration replay. Layout, fixed-cell geometry, whitespace
collapsing, spacing arithmetic, wrapping, alignment, overflow, clipping,
scrolling, opacity, capture, hit testing, semantics, and source order retain
their current owners.

This remains a horizontal-tb, fixed-cell, local software-raster contract. It
does not claim full CSS cascade layers, multiple origins, `!important` layer
inversion, `@import`/`@media`/`@scope`, nested or anonymous layers, CSS-wide
keyword machinery for every property, layer statements, CSS-wide selector
grammar, font metrics, shaping, bidi, vertical writing, antialiasing,
browser-wide CSS conformance, or browser parity.

## Tradeoffs

- Fifteen named layers and a bounded selector-specificity field keep parsing
  and cascade metadata finite, but reject unusually large stylesheets instead
  of silently degrading their order.
- Encoding the private layer rank into the existing cascade comparison avoids
  duplicating every supported property's cascade storage. It makes layer
  precedence apply consistently to the currently supported declarations, while
  keeping the encoded value entirely internal.
- Only top-level named blocks are accepted. This gives deterministic ownership
  and useful rollback semantics now, at the cost of rejecting common nested
  layer syntax until a dedicated grammar boundary is designed.
- `revert-layer` is fully meaningful for one inherited property in this slice,
  but other properties continue to report unsupported values. This prevents an
  incomplete general keyword implementation from changing unrelated behavior.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Pending. The implementation must add the bounded layer parser/registry, private
cascade priority encoding, `revert-layer` candidate rollback, and focused
parser/cascade plus display-list/raster regressions without changing package
dependencies, feature defaults, crate boundaries, or unrelated style owners.

## Verification

The focused gate must cover:

- named-layer parsing, repeated-name reopening, first-appearance ordering,
  layer-vs-specificity precedence, unlayered/inline precedence, and layer
  limit/invalid-form diagnostics;
- case-insensitive `revert-layer` parsing and rejection for unrelated
  properties;
- rollback from a named layer to the preceding named layer, rollback from the
  unlayered bucket to the highest named layer, repeated rollback, and root
  inherited fallback;
- display-list and decoded-raster evidence showing the resolved finite value
  rather than an unresolved keyword.

Then run the established native feature library/integration suites, locked
`glass-dev` tests, strict Clippy, no-default-feature Clippy, warnings-denied
rustdoc, locked package/fuzz/deny/audit checks, and the repository's static
documentation/reliability/adapter/Web IR validators. Record exact commands,
counts, durations, warnings, commit checkpoints, and cleanup evidence here.
Remote CI, browser parity, release, registry publication, and a third crate
remain outside local task evidence unless separately executed and verified.

## Cleanup

All expensive gates must use an isolated task target where practical. Before
deleting generated output, verify no Cargo/rustc/rustdoc/Clippy/fuzz/test
writer owns it and no open file handle remains. Remove only the exact task
target, reports, scratch entries, and generated repository evidence created by
this task; preserve source, durable fixtures, active processes, and unrelated
workloads. Final evidence must show no repository `target/`, no current task
target/report candidates, no open handles, and the before/after available-byte
delta.

## Certification

Pending local certification after implementation, full validation, issue #40
synchronization, and exact regenerable-output cleanup.
