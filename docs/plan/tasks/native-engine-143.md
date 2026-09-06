---
id: native-engine-143
scope: glass-browser/native-engine/cascade-layers-local-dimensions-revert-layer
status: complete
depends-on: [native-engine-142]
---

# Native bounded local dimensions `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven through native-engine-142 to
add standalone, case-insensitive `revert-layer` to the existing local
`width`, `height`, `min-width`, `max-width`, `min-height`, and `max-height`
declarations. The six owners must resolve independently while preserving the
box-model, normal-flow, flex, overflow, capture, hit-test, display-list,
raster, and semantic/source-order contracts.

## Context

The native engine already accepts finite non-negative integer-pixel dimensions
for these six local properties and carries them through the shared box-model
and layout owners. Their stylesheet and inline paths currently keep one
winning concrete candidate, so a higher-priority rollback cannot expose a
lower candidate or the absent local fallback. This slice adds only private
declaration/candidate state and reuses the bounded 15-layer registry and
generic local resolver; it does not add percentage, intrinsic, or negative
sizing.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/CSS2/visudet.html#propdef-width>
- <https://www.w3.org/TR/CSS2/visudet.html#propdef-height>
- <https://www.w3.org/TR/css-sizing-3/#min-width>
- <https://www.w3.org/TR/css-sizing-3/#max-width>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-142.md`

## Contract

### Declaration and cascade state

- Each of `width`, `height`, `min-width`, `max-width`, `min-height`, and
  `max-height` accepts one standalone, case-insensitive `revert-layer` token
  in addition to its existing finite non-negative integer-pixel grammar. The
  rollback representation is private; each public computed field remains an
  `Option<u32>`.
- Each property gets an independent bounded candidate sequence. Rollback
  blocks only the current bounded layer for that property and resolves through
  its lower concrete candidate; if no concrete candidate remains, it resolves
  to the existing absent local fallback (`None`).
- Existing first-appearance named-layer order remains authoritative.
  Specificity and source order decide ties inside a layer; unlayered and inline
  declarations remain above named layers. Repeated rollback, same-block valid
  declarations, and invalid-later-declaration preservation remain explicit
  behavior for every property.
- `!important` is stripped by the existing bounded parser and is not promoted
  to a separate origin; no generic CSS-wide keyword engine is introduced.

### Existing owners preserved

- `width` and `height` continue to feed the existing content/outer box
  calculation and explicit-size precedence.
- `min-width`, `max-width`, `min-height`, and `max-height` continue to feed the
  existing bounded constraint normalization and final layout clamps without
  changing their interaction with padding, margin, box sizing, flex sizing, or
  normal flow.
- The resolved options continue through the existing box rectangles,
  normal-flow/flex placement, root overflow, display-list, raster, capture,
  point hit testing, and semantic/source-order owners without a new geometry or
  artifact schema.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary
  changes.

The slice remains fixture-relative, horizontal-tb, fixed-cell, and bounded. It
does not add percentages, negative dimensions, `auto`/intrinsic sizing,
aspect-ratio, replaced-element sizing, margin collapsing, flex intrinsic
sizing, grid sizing, multiple origins, animation, script, or browser-wide CSS
sizing parity.

## Tradeoffs

- Reusing one generic private local resolver avoids six property-specific
  rollback loops, while six property-local candidate arrays keep each
  `Option<u32>` fallback independent.
- Grouping width/height with min/max constraints reduces checkpoint and
  static-audit overhead, but the focused regression must cover both direct box
  dimensions and constraint interactions so a cascade-only assertion cannot
  hide a geometry regression.
- Keeping the public optional dimension fields unchanged preserves downstream
  build stability at the cost of intentionally excluding wider CSS sizing
  grammar and intrinsic layout behavior.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Implementation

The implementation is complete in
`b55751da4ab36d8bdb840d05ba3185ad473ba056`; the design checkpoint is
`edc29d7c8820a7c7eb6907fe6133ee932df6327f`. The stylesheet and inline
declaration paths now keep independent private bounded candidate sequences for
the six local dimension owners, accept standalone case-insensitive
`revert-layer`, preserve earlier valid values when later declarations are
invalid, and resolve through absent `Option<u32>` local fallbacks. The existing
box-model, normal-flow, flex, overflow, capture, hit-test, display-list,
raster, and semantic/source-order consumers remain unchanged. No public
computed-style field, display-list command, raster schema, diagnostic
transport, dependency, feature default, or crate boundary changed.

## Verification

The completed gate covered:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, unsupported, percentage, negative, and intrinsic forms for
  all six properties;
- independent named-layer priority, repeated rollback, unlayered/inline
  precedence, same-block order, absent fallback, and invalid-later preservation
  for every dimension owner;
- direct width/height box geometry, min/max constraint interaction, normal-flow
  and flex consumers, root overflow/capture, display-list/raster output, point
  hit testing, and unchanged semantic/source order;
- absence of false unsupported-value diagnostics and no public rollback
  keyword leakage; and
- focused `glass-browser` check, targeted behavioral tests, full native
  integration/library tests, strict affected-package Clippy, formatting, and
  final static documentation gates. Remote CI remains unclaimed until an
  explicitly authorized push.

## Evidence

- Focused locked `glass-browser` check passed.
- Local-dimension parser/cascade tests passed 2/2.
- Targeted native integration test passed 1/1 with 179 filtered.
- Full native integration passed 180/180.
- Feature-enabled `glass-browser` library tests passed 951, with 1 ignored.
- Strict affected-package Clippy passed with warnings denied.
- Static validators passed: 557 Markdown documents, 83 current-document
  records, 57 previous-version references, 652 semantic-audit hits, and zero
  current-claim failures. Coverage passed with 345 full-product MCP tools (100
  browser-only), 17 examples, and 22 public modules. Documentation depth
  passed with 93 current guides and 19 substantive contracts. Feature parity
  passed for 14 capabilities across 4 targets (baseline 0.3.0, next 0.3.14,
  checkout 0.3.14). TUI shortcut parity passed with 15 implementation help
  keys and 63 documentation markers. Public read-only adapters passed with 5
  adapters. Reliability passed with 6 scenarios across 4 targets. Web IR
  corpus passed with 8 fixtures, 8 scenarios, and 11 categories. Formatting
  and diff checks passed.
- The isolated target was inventoried and removed only after checking for
  active Cargo/Rust processes and open handles: 5,691,647,750 logical bytes,
  6,362 files, and 925 directories. The audit directory was 4,742 bytes.
  `/tmp` availability increased from 78,191,493,120 to 83,903,422,464 bytes,
  a measured reclaim of 5,711,929,344 bytes. Post-delete checks found neither
  exact path.
- Remote CI remains pending because this checkout is local-only.
