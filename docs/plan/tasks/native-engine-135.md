---
id: native-engine-135
scope: glass-browser/native-engine/cascade-layers-flex-item-order-sizing-revert-layer
status: planned
depends-on: [native-engine-134]
---

# Native bounded flex-item order and sizing `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven by native-engine-134 to add
the explicit CSS-wide `revert-layer` keyword to the existing non-inherited
flex-item owners for `order`, `flex-grow`, `flex-shrink`, and `flex-basis`.
Preserve visual ordering, flex length allocation, min/max freezes, line
formation, source order, overflow, hit testing, display-list coordinates, and
raster output.

## Context

The native engine already accepts bounded finite values for these four
non-inherited properties. `order` controls stable visual sorting; `flex-grow`
and `flex-shrink` allocate positive and negative main-axis free space; and
`flex-basis` supplies the base length consumed by that sizing path. Each owner
has a distinct native fallback (`0`, `0`, `1`, and `auto`) and none inherits
from a parent. This slice adds private declaration-only rollback state while
keeping those defaults and the shared sizing pipeline intact.

Normative references:

- <https://www.w3.org/TR/css-flexbox-1/#order-property>
- <https://www.w3.org/TR/css-flexbox-1/#flex-grow-property>
- <https://www.w3.org/TR/css-flexbox-1/#flex-shrink-property>
- <https://www.w3.org/TR/css-flexbox-1/#flex-basis-property>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-134.md`

## Contract

### Declaration and cascade state

- Each of `order`, `flex-grow`, `flex-shrink`, and `flex-basis` accepts one
  standalone, case-insensitive `revert-layer` token. The token exists only in
  private declaration/candidate state; the existing finite public value types
  remain unchanged.
- Stylesheet candidates use the existing bounded first-appearance named-layer
  order. Selector specificity and source order decide ties within a layer;
  unlayered and inline declarations remain above named layers.
- A winning rollback blocks only its current layer and selects the highest
  priority remaining concrete candidate. Repeated rollback continues through
  lower layers and then that property's non-inherited native fallback.
  Unlayered/inline rollback can expose the highest named candidate.
- The properties remain non-inherited. A descendant or flex item with no local
  candidate uses its own fallback, even when an ancestor has a different
  declaration.
- `flex` remains the existing bounded finite shorthand. Its concrete grow,
  shrink, and basis components are stored as private `Value` forms; this slice
  does not accept `flex: revert-layer` or add CSS-wide shorthand expansion.
  Existing declaration-order behavior between the finite shorthand and its
  longhands remains unchanged within one declaration block.
- Valid rollback declarations produce no false unsupported-value diagnostic.
  Other CSS-wide keywords, multiple origins, `!important` inversion, layer
  statements, nested/anonymous/comma layers, animation, script, percentage or
  intrinsic sizing, grid, writing-mode conformance, and browser-wide Flexbox
  conformance remain outside the contract.

### Existing owners preserved

Resolved values continue through stable visual `(order, source_index)` sorting,
flex base-size selection, grow allocation, shrink deficit freezing and
redistribution, min/max constraints, wrap and reverse line formation, gaps,
alignment, overflow and root-scroll projection, point hit testing,
display-list generation, software rasterization, and semantic/source-order
owners. No public computed-style or layout/display-list schema, dependency,
feature default, or crate boundary changes are permitted.

The slice remains a horizontal-tb, fixed-cell, bounded local-content contract.
It changes which existing flex-item declaration wins; it does not add
intrinsic or percentage sizing, grid, general event behavior, or browser
parity.

## Tradeoffs

- Four property-local declaration enums and candidate arrays add repeated
  cascade state, but preserve finite public values and make rollback explicit
  at each sizing/order owner.
- One family boundary shares the cold build and layer-resolution proof across
  visual ordering and three sizing paths, while the fixture must distinguish
  sorting from length allocation and preserve authored text order.
- Keeping finite `flex` expansion concrete avoids silently inventing shorthand
  reset semantics or changing existing declaration-order behavior.
- The test matrix asserts local defaults separately from inherited `direction`
  and from parent layout so rollback cannot leak ancestor or sibling state.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The focused gate must cover:

- case-insensitive standalone parsing and typed rejection of other CSS-wide,
  mixed, malformed, and finite-shorthand rollback forms for all four
  properties;
- named-layer ordering before specificity, repeated rollback,
  unlayered/inline precedence, declaration-order interaction with finite
  `flex`, and each local fallback;
- stable visual order with source/semantic order preserved;
- grow allocation, shrink deficit handling, flex-basis base-size selection,
  min/max constraint behavior, and their existing wrap/direction/alignment
  consumers;
- shared layout, display-list, hit-test, overflow/scroll, and decoded-raster
  coordinates for rolled-back order and sizing;
- no false unsupported-value diagnostics and no public declaration-keyword
  leakage.

Targeted checks follow the completed family behavioral unit. Full native,
two-crate, strict, fuzz/security, and static documentation gates are final
validation only; remote CI remains unclaimed until an explicitly authorized
push.
