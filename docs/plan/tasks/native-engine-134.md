---
id: native-engine-134
scope: glass-browser/native-engine/cascade-layers-flexbox-alignment-revert-layer
status: planned
depends-on: [native-engine-133]
---

# Native bounded Flexbox `revert-layer` family

## Objective

Reuse the bounded cascade-layer machinery proven by native-engine-133 to add
the explicit CSS-wide `revert-layer` keyword to the existing non-inherited
Flexbox owners for `flex-wrap`, `justify-content`, `align-items`, `align-self`,
and `align-content`. Preserve line formation, free-space distribution,
cross-axis item and line placement, source order, overflow, hit testing,
display-list coordinates, and raster output.

## Context

The native engine already accepts finite values for these five non-inherited
properties. They feed distinct shared owners: `flex-wrap` forms lines;
`justify-content` distributes main-axis free space; `align-items` and
`align-self` place complete item subtrees across a line; and `align-content`
places or stretches complete formed lines. A missing local declaration uses
each property's existing native fallback and never inherits from a parent.
This family adds private declaration-only rollback state while preserving those
defaults and owner boundaries.

Normative references:

- <https://www.w3.org/TR/css-flexbox-1/#flex-wrap-property>
- <https://www.w3.org/TR/css-flexbox-1/#justify-content-property>
- <https://www.w3.org/TR/css-flexbox-1/#align-items-property>
- <https://www.w3.org/TR/css-flexbox-1/#align-self-property>
- <https://www.w3.org/TR/css-flexbox-1/#align-content-property>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-133.md`

## Contract

### Declaration and cascade state

- Each of `flex-wrap`, `justify-content`, `align-items`, `align-self`, and
  `align-content` accepts one standalone, case-insensitive `revert-layer`
  token. The token exists only in private declaration/candidate state; each
  existing finite public computed enum remains unchanged.
- Stylesheet candidates use the existing bounded first-appearance named-layer
  order. Selector specificity and source order decide ties within a layer;
  unlayered and inline declarations remain above named layers.
- A winning rollback blocks only its current layer and selects the highest
  priority remaining concrete candidate. Repeated rollback continues through
  lower layers and then that property's existing non-inherited fallback.
  Unlayered/inline rollback can expose the highest named candidate.
- The properties remain non-inherited. A descendant with no local candidate
  uses its own native fallback, even when its parent has a different computed
  Flexbox value. Local rollback with no remaining candidate uses the same
  fallback.
- `flex-flow` and `place-content` remain the existing bounded finite
  shorthands. Their concrete components are stored as private `Value` forms;
  this slice does not accept `flex-flow: revert-layer`,
  `place-content: revert-layer`, or add CSS-wide shorthand expansion.
  Existing declaration-order behavior between each finite shorthand and its
  longhands remains unchanged within one declaration block.
- Valid rollback declarations produce no false unsupported-value diagnostic.
  Other CSS-wide keywords, multiple origins, `!important` inversion, layer
  statements, nested/anonymous/comma layers, animation, script, grid,
  writing-mode conformance, intrinsic/percentage sizing, and browser-wide
  Flexbox conformance remain outside the contract.

### Existing owners preserved

Resolved values continue through the existing flex line formation,
wrap-reverse mapping, main-axis free-space distribution, gap and auto-margin
handling, flex sizing, cross-axis item alignment, cross-line distribution and
stretch, overflow and root-scroll projection, point hit testing, display-list
generation, software rasterization, and semantic/source-order owners. No public
computed-style or layout/display-list schema, dependency, feature default, or
crate boundary changes are permitted.

The slice remains a horizontal-tb, fixed-cell, bounded local-content contract.
It changes which existing Flexbox declaration wins; it does not add intrinsic
sizing, percentage sizing, grid, general event behavior, or browser parity.

## Tradeoffs

- Five property-local declaration enums and candidate arrays add repeated
  cascade state, but make each CSS-wide keyword private and keep all public
  computed values finite.
- One family boundary shares the cold build and layer-resolution proof across
  five non-inherited owners, while the fixture must assert separate fallback
  and owner behavior for each property.
- Keeping finite `flex-flow` and `place-content` expansion concrete avoids
  silently inventing shorthand reset semantics or changing existing
  declaration-order behavior.
- The test matrix distinguishes local non-inheritance from inherited
  `direction`, so a resolver cannot accidentally copy parent Flexbox state.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The focused gate must cover:

- case-insensitive standalone parsing and typed rejection of other CSS-wide,
  mixed, malformed, and finite-shorthand rollback forms for all five
  properties;
- named-layer ordering before specificity, repeated rollback,
  unlayered/inline precedence, declaration-order interaction with finite
  `flex-flow` and `place-content`, and each local default fallback;
- non-inheritance to descendants and preservation of row/column/reverse line
  formation, wrapped and wrap-reverse geometry, main-axis justification,
  item-level cross-axis alignment, line-level cross-axis distribution, gaps,
  margins, and flex sizing;
- source-order and semantic-order preservation while visual geometry changes;
- shared layout, display-list, hit-test, overflow/scroll, and decoded-raster
  coordinates for rolled-back flex placement;
- no false unsupported-value diagnostics and no public declaration-keyword
  leakage.

Targeted checks follow the completed family behavioral unit. Full native,
two-crate, strict, fuzz/security, and static documentation gates are final
validation only; remote CI remains unclaimed until an explicitly authorized
push.
