---
id: native-engine-136
scope: glass-browser/native-engine/cascade-layers-flex-shorthand-revert-layer
status: planned
depends-on: [native-engine-135]
---

# Native bounded flex shorthand `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven by native-engine-135 to add
the explicit CSS-wide `revert-layer` keyword to the existing finite `flex`
shorthand. The shorthand rollback must act on the existing non-inherited
`flex-grow`, `flex-shrink`, and `flex-basis` component candidates without
changing their public computed values or the shared sizing and artifact owners.

## Context

The native engine already expands bounded finite `flex` forms into the three
component declarations. native-engine-135 adds private rollback state to those
components, but intentionally leaves `flex: revert-layer` outside the
contract. This slice makes the shorthand equivalent to a simultaneous
component-local rollback at the declaration's bounded cascade layer. The
existing 15 named layers plus the unlayered/inline bucket remain the only
cascade boundary.

Normative references:

- <https://www.w3.org/TR/css-flexbox-1/#flex-property>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-135.md`

## Contract

### Shorthand declaration and cascade state

- `flex: revert-layer` accepts one standalone, case-insensitive token. It is
  represented only by the existing private component declaration enums; no
  public computed-style keyword or new display-list field is introduced.
- The shorthand rollback writes `RevertLayer` to grow, shrink, and basis as
  one declaration. A later valid longhand in the same declaration block
  overrides only its component; a later valid shorthand resets all three
  components. Invalid declarations remain ignored and do not erase an earlier
  valid shorthand or longhand.
- Stylesheet candidates retain the existing bounded first-appearance named
  layer order. Selector specificity and source order decide ties inside a
  layer; unlayered and inline declarations remain above named layers.
- A winning shorthand rollback blocks only its current layer for each
  component and resolves each component independently through lower concrete
  candidates. Repeated rollback reaches each component's native fallback:
  `flex-grow: 0`, `flex-shrink: 1`, and `flex-basis: auto`.
- The shorthand remains non-inherited. A descendant or flex item without a
  local component candidate uses its own fallback, regardless of an
  ancestor's shorthand.
- Existing finite shorthand forms (`none`, `auto`, integer factors, and
  bounded pixel/`auto` bases) remain concrete `Value` components with the
  established declaration-order behavior.
- Other CSS-wide keywords, multiple origins, `!important` inversion, layer
  statements, nested/anonymous/comma layers, animation, script, percentage or
  intrinsic sizing, grid, writing-mode conformance, and browser-wide Flexbox
  conformance remain outside the contract.

### Existing owners preserved

Resolved components continue through stable visual ordering, flex base-size
selection, grow allocation, shrink deficit freezing and redistribution,
min/max constraints, row/column and wrap/reverse line formation, gaps,
alignment, overflow and root-scroll projection, point hit testing,
display-list generation, software rasterization, and semantic/source-order
owners. No public computed-style or layout/display-list schema, dependency,
feature default, or crate boundary changes are permitted.

The slice remains a horizontal-tb, fixed-cell, bounded local-content contract.
It changes only which existing flex components win the cascade; it does not
add intrinsic or percentage sizing, grid, general event behavior, or browser
parity.

## Planned implementation

- Add a declaration-aware finite `flex` shorthand parser that recognizes the
  standalone case-insensitive rollback token and otherwise delegates to the
  existing concrete shorthand parser.
- Feed the three resulting private component declarations through the current
  stylesheet and inline candidate arrays, keeping component-level source-order
  overrides intact.
- Extend parser, diagnostic, cascade, layout, display-list, hit-test, and
  decoded-raster regressions to cover named-layer, repeated, unlayered, inline,
  fallback, and same-block longhand/shorthand precedence cases.

## Tradeoffs

- Reusing the three existing private component types avoids a fourth public
  shorthand state and keeps the cold implementation small, at the cost of
  testing synchronized component writes explicitly.
- Treating shorthand rollback as three component candidates matches the
  existing finite shorthand expansion and keeps lower-layer resolution
  independent when one component has a concrete candidate that another lacks.
- Same-block source-order behavior remains encoded by parser assignment order;
  this preserves current behavior but makes the regression fixture responsible
  for guarding all three component overrides.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The focused gate must cover:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, and finite-shorthand forms;
- named-layer priority, repeated rollback, unlayered/inline precedence, each
  local fallback, and independent component resolution;
- declaration-order interaction between `flex`, `flex-grow`, `flex-shrink`,
  and `flex-basis`, including invalid later declarations;
- visual order/source-semantic order, grow/shrink allocation, basis sizing,
  min/max constraints, and the existing direction/wrap/alignment consumers;
- shared layout, display-list, hit-test, overflow/scroll, and decoded-raster
  coordinates; and
- no false unsupported-value diagnostics or public declaration-keyword
  leakage.

Targeted checks follow the completed shorthand behavioral unit. Full native,
two-crate, strict, fuzz/security, and static documentation gates are final
validation only; remote CI remains unclaimed until an explicitly authorized
push.
