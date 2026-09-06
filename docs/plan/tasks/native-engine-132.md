---
id: native-engine-132
scope: glass-browser/native-engine/cascade-layers-direction-revert-layer
status: planned
depends-on: [native-engine-131]
---

# Native bounded `direction: revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven by native-engine-131 to add
the explicit CSS-wide `revert-layer` keyword to the existing inherited
`direction` owner. Preserve logical text-edge mapping, row and column flex
directionality, wrapped-line placement, source/semantic order, hit testing,
display-list coordinates, and raster output.

## Context

The native engine currently accepts only the finite inherited
`direction:ltr|rtl` values. The resolved value already travels through the
DOM style walk and is consumed by text alignment, row reversal, column
cross-axis placement, wrapping, and shared artifact projection. This slice
adds private declaration-only rollback state and per-layer candidates without
changing the public `DirectionValue` or its root `ltr` default.

Normative references:

- <https://www.w3.org/TR/css-writing-modes-4/#propdef-direction>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-131.md`

## Contract

### Declaration and cascade state

- `direction` accepts one standalone, case-insensitive `revert-layer`
  token. The token exists only in private declaration/candidate state;
  `ltr` and `rtl` remain the only public computed values.
- Stylesheet candidates use the existing bounded first-appearance named-layer
  order. Selector specificity and source order decide ties within a layer;
  unlayered and inline declarations remain above named layers.
- A winning rollback blocks only its current layer and selects the highest
  priority remaining concrete candidate. Repeated rollback continues through
  lower layers and then the inherited direction; unlayered/inline rollback can
  expose the highest named candidate.
- With no candidate remaining, the root result remains `DirectionValue::Ltr`.
  A descendant with no local candidate retains the existing inherited value.
- Valid rollback declarations produce no false unsupported-value diagnostic.
  Other CSS-wide keywords, vertical writing modes, multiple origins,
  `!important` inversion, layer statements, nested/anonymous/comma layers,
  animation, script, and browser-wide writing-mode conformance remain outside
  the contract.

### Existing owners preserved

The resolved direction continues through the existing inherited style walk,
logical `text-align:start|end` mapping, row/row-reverse physical placement,
column/column-reverse cross-axis placement, wrapped-line mapping, overflow and
root-scroll projection, point hit testing, display-list generation, software
rasterization, and semantic/source-order owners. No public computed-style or
layout/display-list schema, dependency, feature default, or crate boundary
changes are permitted.

The slice remains a horizontal-tb, fixed-cell, bounded local-content
contract; it does not claim full writing-mode, bidi, shaping, or browser
parity.

## Implementation

The implementation will add a private `DirectionDeclaration` enum with
`Value(DirectionValue)` and `RevertLayer` variants, store winning stylesheet
and inline candidates in the existing bounded layer array, and resolve them
before the inherited style walk feeds the existing direction consumers. The
diagnostic classifier and declaration parser will share one standalone
`revert-layer`-aware parser so accepted case variants cannot be reported as
unsupported.

## Tradeoffs

- A property-local declaration enum and fixed-size candidate array add a small
  amount of cascade state while keeping the public direction value finite and
  avoiding generic CSS-wide keyword machinery.
- Testing direction through text, row, column, and wrapped-line owners gives
  high leverage for a one-property slice, but does not broaden the engine into
  full bidi or writing-mode support.
- Reusing the existing resolver keeps build and runtime cost bounded, while
  retaining the explicit 15-layer limit and typed diagnostics for unsupported
  forms.
- The implementation intentionally does not reinterpret `direction` as a
  flex ordering or source-order property: rollback changes physical mapping
  only; DOM order, semantic order, and authored text order remain unchanged.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and issue records

## Verification

The focused gate must cover:

- case-insensitive standalone parsing and typed rejection of other CSS-wide,
  mixed, malformed, and unsupported direction forms;
- named-layer ordering before specificity, repeated rollback,
  unlayered/inline precedence, inherited descendants, and root `ltr` fallback;
- logical text start/end placement under both directions;
- row and column flex directionality, wrapped-line physical mapping, and
  source-order preservation;
- shared layout, display-list, hit-test, overflow/scroll, and decoded-raster
  coordinates where direction changes physical placement;
- no false unsupported-value diagnostics and no public declaration-keyword
  leakage.

Targeted checks follow the completed behavioral unit. Full native,
two-crate, strict, fuzz/security, and static documentation gates are final
validation only; remote CI remains unclaimed until an explicitly authorized
push.
