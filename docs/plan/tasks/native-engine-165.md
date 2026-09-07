---
id: native-engine-165
scope: glass-browser/native-engine/cascade-color-current-color
status: planned
depends-on: [native-engine-164]
---

# Native bounded `color: currentColor`

## Objective

Accept the exact case-insensitive `currentColor` keyword for the existing
inherited `color` property. Resolve a winning local `color: currentColor`
declaration from the already-computed inherited color, or from the bounded
initial black fallback when no inherited color is present. Preserve the public
`Option<NativeColor>` value and every existing background, border,
text-decoration, glyph, display-list, capture, and raster consumer.

This slice closes the self-reference gap deliberately left outside
native-engine-163 and -164 without introducing a generic CSS value-dependency
graph.

## Context

The native engine now resolves `currentColor` for physical border colors,
`background-color`, and local `text-decoration-color` after the existing
element/inherited `color` owner is selected. The remaining common gap is the
`color` property itself. A local `color: currentColor` must not recursively
resolve against its own unresolved declaration: its bounded meaning here is
the inherited computed color, with the existing initial black fallback at the
root.

Normative references:

- <https://www.w3.org/TR/css-color-4/#currentcolor-color>
- <https://www.w3.org/TR/css-color-4/#the-color-property>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-164.md`
- `docs/plan/tasks/native-engine-163.md`

## Contract

### Declaration and cascade state

- `color` accepts every existing bounded literal/alpha color, exact
  case-insensitive `currentColor`, and standalone case-insensitive
  `revert-layer`.
- A private deferred color value participates in the existing named-layer,
  unlayered, inline, specificity, source-order, same-block, and
  valid-before-invalid cascade behavior. It resolves only after the parent
  computed style has supplied inherited color.
- A local concrete color remains the winning element color when selected. A
  local `currentColor` resolves to the inherited computed color; when that is
  absent, it resolves to the bounded initial black value. A missing local
  declaration retains the existing inherited `Option<NativeColor>` behavior,
  including `None` at an undeclared root.
- Because `currentColor` on `color` is resolved from the parent value, the
  implementation does not recurse through the local declaration and does not
  create a cyclic dependency. Descendants inherit the resulting concrete
  color through the existing DOM style walk.
- Malformed values, other CSS-wide keywords, gradients, image functions,
  system colors, color-space functions, percentages, arbitrary functions,
  custom properties, and unresolved declaration keywords remain typed
  unsupported-value diagnostics and do not replace an earlier valid
  declaration.

### Existing owners preserved

- `NativeColorValue::CurrentColor` (or an equivalent private declaration-only
  owner) remains private until computed-style resolution. Public computed style
  continues to expose `Option<NativeColor>`; display commands, capture bytes,
  raster schemas, dependencies, feature defaults, and crate boundaries remain
  unchanged.
- The resolved concrete color continues through existing glyph text paint,
  `background-color: currentColor`, border current-color, decoration
  current-color, clipping, opacity, scrolling, capture, software raster,
  point-hit, and semantic/source-order consumers without downstream keyword
  handling.
- The slice remains fixture-relative, horizontal-tb, integer-pixel,
  fixed-cell, non-table, and feature-gated behind `native-engine` inside
  `glass-browser`.

## Tradeoffs

- A private deferred variant keeps the public color schema stable and reuses
  the already-proven post-cascade resolution boundary. A generic dependency
  graph would add cycle and invalidation complexity outside this engine's
  bounded contract.
- Resolving a local `currentColor` from the parent computed value matches the
  needed self-reference behavior while making the root initial-black choice
  explicit. It preserves the distinction between an omitted root color
  (`None`) and an explicit root `currentColor` (`Some(black)`).
- The same concrete result feeds all current-color consumers, so one test can
  verify that the feature composes across glyph, background, border, and
  decoration paint without widening any artifact schema.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive parser acceptance for literal, alpha, `currentColor`, and
  `revert-layer`, plus typed rejection of unsupported and unrelated forms;
- local concrete, local current-color, inherited, explicit root black
  fallback, omitted-root `None`, layer/unlayered/inline precedence,
  specificity, same-block order, and valid-before-invalid preservation;
- non-recursive descendant inheritance and composition of the resolved color
  through glyph, background, border, and decoration display/raster artifacts,
  including clipping, opacity, capture, point-hit, semantic/source order, and
  private/public separation; and
- focused feature check/tests, full native integration/library tests, strict
  affected-package Clippy, rustdoc, paired-crate check/build, packaging,
  formatting, documentation, final static gates, workspace all-target/
  all-feature testing, and bounded regenerable-target cleanup. Remote CI
  remains unclaimed until an explicitly authorized push.

## Implementation

Pending. This docs-first contract records the bounded inherited-color
resolution boundary before the parser and computed-style resolver are changed.
