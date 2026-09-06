---
id: native-engine-138
scope: glass-browser/native-engine/cascade-layers-gap-revert-layer
status: planned
depends-on: [native-engine-137]
---

# Native bounded gap-family `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven by native-engine-137 to add
standalone, case-insensitive `revert-layer` to the existing finite `gap`,
`row-gap`, and `column-gap` declarations. The shorthand and longhands must
resolve each physical gap component independently while preserving the
already-certified flex item/line geometry and all downstream artifacts.

## Context

The native engine already parses finite non-negative integer-pixel `gap`,
`row-gap`, and `column-gap` values and expands the shorthand into its row and
column components. The current cascade helper keeps shorthand/longhand source
order, selector specificity, unlayered precedence, and inline precedence in
one concrete candidate. This slice changes only the private declaration state
needed to roll back a winning component to lower bounded candidates.

Normative references:

- <https://www.w3.org/TR/css-align-3/#gap-properties>
- <https://www.w3.org/TR/css-flexbox-1/#gap-properties>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-137.md`

## Contract

### Declaration and cascade state

- `gap: revert-layer`, `row-gap: revert-layer`, and
  `column-gap: revert-layer` accept one standalone, case-insensitive token.
  They are represented only by private component declaration state; the
  public computed style remains `u32` values and no new artifact field is
  introduced.
- `gap: revert-layer` writes rollback candidates to both the row and column
  components. `row-gap` and `column-gap` write rollback to only their matching
  component.
- A later valid longhand in the same declaration block overrides only its
  component; a later valid `gap` shorthand resets both components. Invalid
  declarations remain ignored and do not erase an earlier valid declaration.
- The existing bounded first-appearance named-layer order remains authoritative.
  Specificity and source order decide ties inside a layer; unlayered and inline
  declarations remain above named layers.
- A winning rollback blocks only its current bounded layer for that physical
  component and resolves through lower concrete shorthand/longhand candidates
  or the native fallback of `0`.
- The gap family remains non-inherited. Descendants without local candidates
  retain a zero local fallback regardless of an ancestor's gap.
- Existing finite one- and two-value `gap` forms and finite longhands remain
  concrete component values, including established shorthand/longhand source
  order and row/column flex-axis mapping.
- Other CSS-wide keywords, multiple origins, `!important` inversion, layer
  statements, nested/anonymous/comma layers, percentages, negative or
  fractional lengths, grid conformance, and browser-wide gap conformance
  remain outside the contract.

### Existing owners preserved

Resolved row and column gaps continue through row/column main-axis placement,
wrapped-line formation, cross-line distribution, overflow and root-scroll
projection, point hit testing, display-list generation, software
rasterization, and semantic/source-order owners. No public computed-style or
layout/display-list schema, dependency, feature default, or crate boundary
changes are permitted.

The slice remains a horizontal-tb, fixed-cell, bounded local-content contract.
It changes only which existing gap component candidates win the cascade; it
does not add intrinsic or percentage sizing, grid behavior, or browser parity.

## Planned implementation

- Add private declaration-aware parsers for the standalone rollback token while
  retaining the existing concrete gap parsers.
- Preserve the existing shorthand/longhand declaration-order comparison while
  retaining a bounded candidate per physical component and cascade layer.
- Extend parser, diagnostic, row/column layout, display-list, hit-test,
  overflow/scroll, and decoded-raster regressions.

## Tradeoffs

- Expanding the shorthand into two private component candidates avoids a public
  shorthand state and makes rollback semantics match the already-proven
  component resolver, at the cost of four small fixed candidate arrays in the
  style walk.
- Keeping the existing `GapCascadeValue` precedence tuple preserves current
  same-block behavior, but resolution must explicitly merge shorthand and
  longhand candidates before applying layer rollback.
- Retaining finite integer-pixel parsing keeps compilation and runtime bounded,
  while CSS-wide semantics beyond `revert-layer` and percentage resolution
  remain intentionally typed as unsupported.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The focused gate must cover:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, and finite forms;
- named-layer priority, repeated rollback, unlayered/inline precedence,
  independent row/column fallback, and same-block shorthand/longhand order;
- row and column placement, wrapping, line distribution, overflow/scroll, and
  source/semantic order through the existing layout owner;
- hit testing, display-list geometry, and decoded-raster coordinates; and
- no false unsupported-value diagnostics or public declaration-keyword
  leakage.

Targeted checks follow the completed gap behavioral unit. Full native,
two-crate, strict, fuzz/security, and static documentation gates are final
validation only; remote CI remains unclaimed until an explicitly authorized
push.
