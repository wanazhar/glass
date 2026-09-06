---
id: native-engine-137
scope: glass-browser/native-engine/cascade-layers-flex-flow-place-content-revert-layer
status: planned
depends-on: [native-engine-136]
---

# Native bounded `flex-flow` and `place-content` `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven by native-engine-136 to add
standalone, case-insensitive `revert-layer` to the existing finite
`flex-flow` and `place-content` shorthands. Each shorthand must roll back only
its own component candidates while preserving the already-certified direction,
wrapping, line alignment, main-axis distribution, and artifact owners.

## Context

The native engine already expands finite `flex-flow` into `flex-direction` and
`flex-wrap`, and finite `place-content` into `align-content` and
`justify-content`. native-engine-133 and native-engine-134 add private rollback
state to those four longhands, while native-engine-136 proves shorthand
rollback for the flex-item sizing family. This slice applies the same
component-expansion rule to the two remaining bounded Flexbox shorthands.

Normative references:

- <https://www.w3.org/TR/css-flexbox-1/#flex-flow-property>
- <https://www.w3.org/TR/css-align-3/#place-content-property>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-136.md`

## Contract

### Shorthand declaration and cascade state

- `flex-flow: revert-layer` and `place-content: revert-layer` accept one
  standalone, case-insensitive token. They are represented only by the
  existing private longhand declaration enums; no public computed-style
  keyword or new display-list field is introduced.
- `flex-flow: revert-layer` writes `RevertLayer` to `flex-direction` and
  `flex-wrap` as one declaration. `place-content: revert-layer` writes
  `RevertLayer` to `align-content` and `justify-content` as one declaration.
- A later valid longhand in the same declaration block overrides only its
  component; a later valid shorthand resets all of its components. Invalid
  declarations remain ignored and do not erase an earlier valid declaration.
- Stylesheet candidates retain the existing bounded first-appearance named
  layer order. Selector specificity and source order decide ties inside a
  layer; unlayered and inline declarations remain above named layers.
- A winning shorthand rollback blocks only its current bounded layer for each
  component and resolves independently through lower concrete candidates and
  the existing native fallback: `row`/`nowrap` for `flex-flow`, and
  `flex-start` for both `place-content` components.
- The shorthands remain non-inherited. Descendants without local candidates
  retain their own local fallbacks, regardless of an ancestor's shorthand.
- Existing finite `flex-flow` and `place-content` forms remain concrete
  component values, including omitted-component expansion and established
  same-block longhand precedence.
- Other CSS-wide keywords, multiple origins, `!important` inversion, layer
  statements, nested/anonymous/comma layers, animation, script, grid,
  writing-mode conformance, intrinsic or percentage sizing, and browser-wide
  Flexbox conformance remain outside the contract.

### Existing owners preserved

Resolved components continue through physical row/column/reverse mapping,
line formation, main-axis justification, item and line cross-axis alignment,
gaps, overflow and root-scroll projection, point hit testing, display-list
generation, software rasterization, and semantic/source-order owners. No public
computed-style or layout/display-list schema, dependency, feature default, or
crate boundary changes are permitted.

The slice remains a horizontal-tb, fixed-cell, bounded local-content contract.
It changes only which existing shorthand components win the cascade; it does
not add intrinsic or percentage sizing, grid, general event behavior, or
browser parity.

## Planned implementation

- Add declaration-aware finite shorthand parsers for the standalone rollback
  token while retaining the existing concrete parsers.
- Feed the resulting private longhand declarations through the current
  stylesheet and inline candidate arrays, preserving component-level source
  order and invalid-declaration behavior.
- Extend parser, diagnostic, cascade, layout, display-list, hit-test, and
  decoded-raster regressions across direction/wrap and line/main-axis
  placement.

## Tradeoffs

- Reusing the four existing private longhand types avoids new public shorthand
  state and keeps rollback resolution component-local, at the cost of explicit
  synchronized-write tests for each shorthand.
- Combining the two shorthands in one slice reduces duplicate compile and
  documentation overhead, while the fixture must still separate their
  independent fallbacks and artifact owners.
- Retaining finite omitted-component expansion preserves current behavior but
  means CSS-wide reset semantics beyond `revert-layer` remain intentionally
  typed as unsupported.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The focused gate must cover:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, and finite shorthand forms;
- named-layer priority, repeated rollback, unlayered/inline precedence, each
  independent component fallback, and same-block longhand/shorthand order;
- direction, wrap, main-axis, and cross-axis behavior through the existing
  layout owner;
- source/semantic order, hit testing, display-list geometry, overflow/scroll,
  and decoded-raster coordinates; and
- no false unsupported-value diagnostics or public declaration-keyword
  leakage.

Targeted checks follow the completed shorthand behavioral unit. Full native,
two-crate, strict, fuzz/security, and static documentation gates are final
validation only; remote CI remains unclaimed until an explicitly authorized
push.
