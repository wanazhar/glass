---
id: native-engine-133
scope: glass-browser/native-engine/cascade-layers-flex-direction-revert-layer
status: complete
depends-on: [native-engine-132]
---

# Native bounded `flex-direction: revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven by native-engine-132 to add
the explicit CSS-wide `revert-layer` keyword to the existing non-inherited
`flex-direction` owner. Preserve row/column placement, direction-aware
physical mapping, wrapping, flex sizing, source order, hit testing,
display-list coordinates, and raster output.

## Context

The native engine already accepts the finite non-inherited
`flex-direction:row|row-reverse|column|column-reverse` grammar. The resolved
value feeds the shared flex main-axis/cross-axis and wrapped-line owners; a
child without a local declaration intentionally uses the native `row` default
rather than inheriting its parent. This slice adds private declaration-only
rollback state and per-layer candidates while preserving that distinction.

Normative references:

- <https://www.w3.org/TR/css-flexbox-1/#flex-direction-property>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-132.md`

## Contract

### Declaration and cascade state

- `flex-direction` accepts one standalone, case-insensitive `revert-layer`
  token. The token exists only in private declaration/candidate state;
  `row`, `row-reverse`, `column`, and `column-reverse` remain the only public
  computed values.
- Stylesheet candidates use the existing bounded first-appearance named-layer
  order. Selector specificity and source order decide ties within a layer;
  unlayered and inline declarations remain above named layers.
- A winning rollback blocks only its current layer and selects the highest
  priority remaining concrete candidate. Repeated rollback continues through
  lower layers and then the non-inherited `row` fallback. Unlayered/inline
  rollback can expose the highest named candidate.
- The property remains non-inherited: a descendant with no local candidate
  uses `row`, even when its parent computes to a different direction. A local
  rollback with no remaining candidate also uses `row`.
- `flex-flow` remains the existing bounded finite shorthand. Its finite
  direction component is stored as a concrete private `Value`; this slice does
  not accept `flex-flow: revert-layer` or add CSS-wide shorthand expansion.
  Existing declaration-order behavior between `flex-flow` and explicit
  `flex-direction` remains unchanged within one declaration block.
- Valid rollback declarations produce no false unsupported-value diagnostic.
  Other CSS-wide keywords, multiple origins, `!important` inversion, layer
  statements, nested/anonymous/comma layers, animation, script, grid,
  writing-mode conformance, and browser-wide Flexbox conformance remain
  outside the contract.

### Existing owners preserved

The resolved direction continues through the existing flex main-axis and
cross-axis placement, row/column and reverse mapping, wrap and wrap-reverse
line formation, gap and auto-margin distribution, flex grow/shrink/basis,
alignment, overflow and root-scroll projection, point hit testing,
display-list generation, software rasterization, and semantic/source-order
owners. No public computed-style or layout/display-list schema, dependency,
feature default, or crate boundary changes are permitted.

The slice remains a horizontal-tb, fixed-cell, bounded local-content
contract. It changes which existing flex-direction owner wins; it does not
add intrinsic sizing, percentage sizing, grid, general event behavior, or
browser parity.

## Implementation and local result

The implementation is complete in `56944c83` (`feat(native-engine): support
flex direction rollback`). It adds private `FlexDirectionDeclaration` state,
stores stylesheet and inline candidates in the existing bounded 15-layer array,
resolves repeated rollback before the non-inherited `FlexDirectionValue::Row`
fallback, and keeps finite `flex-flow` expansion concrete. The supported-value
diagnostic classifier and declaration parser share the same standalone,
case-insensitive `revert-layer` parser, so accepted forms do not produce a
false unsupported-value diagnostic.

The focused and affected-package gates passed:

- `cargo check -p glass-browser --features native-engine --test native_engine
  --locked` passed in the fresh 133 target.
- The `flex_direction` library filter passed 3/3 parser/cascade tests.
- The new non-inherited flex-direction integration regression passed 1/1.
- Full native integration passed 170/170 in 3.11s.
- The affected feature-enabled `glass-browser` library passed 934 tests with
  1 ignored and 0 failures using `RUST_MIN_STACK=33554432`.
- Warnings-denied affected-package Clippy passed; formatting and diff checks
  passed.

The integration fixture verifies named-layer ordering, repeated rollback,
unlayered and inline precedence, finite `flex-flow` interaction, non-inherited
root-row fallback, descendant isolation, row and column placement, wrapped
line formation, source order, hit testing, display-list geometry, and decoded
raster output. A first assertion used an offset from an already-positioned
child rectangle and was corrected to an interior point; no production behavior
changed. No public schema, dependency, feature default, or crate boundary
changed. Full two-crate, strict, fuzz/security, source-built documentation
coverage, and remote-CI gates remain issue-level gates. No remote CI, push,
release, tag, registry publication, or browser-parity claim is made.

The exact regenerable `/tmp/glass-133-focused` target measured
3,052,916,399 logical bytes across 4,253 files and 658 directories. Process
and `/proc` descriptor checks found no consumer after validation; the target
was removed with bounded `find -P ... -xdev -depth -delete`. No repository
target, source, fixture, durable data, or unrelated temporary path was removed.
Available filesystem bytes increased from 81,027,399,680 to
84,093,984,768, a measured delta of 3,066,585,088 bytes.

## Tradeoffs

- A property-local declaration enum and fixed-size candidate array keep the
  public computed enum finite and avoid premature generic CSS-wide keyword
  machinery, at the cost of a small amount of repeated cascade state.
- Testing row, column, reverse, wrapped, direction-aware, and artifact paths
  gives high leverage for a layout property, but does not prove arbitrary
  Flexbox or writing-mode conformance.
- Keeping `flex-flow: revert-layer` out of scope avoids silently inventing
  shorthand reset semantics while preserving the existing finite shorthand
  expansion and declaration-order contract.
- The non-inherited `row` fallback is asserted separately from the inherited
  `direction` fallback so the rollback implementation cannot accidentally
  leak parent flex state into descendants.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The focused gate must cover:

- case-insensitive standalone parsing and typed rejection of other CSS-wide,
  mixed, malformed, and shorthand rollback forms;
- named-layer ordering before specificity, repeated rollback,
  unlayered/inline precedence, declaration-order interaction with finite
  `flex-flow`, and local root-`row` fallback;
- non-inheritance to descendants and existing row/row-reverse,
  column/column-reverse, wrap, reverse, gap, sizing, margin, alignment, and
  direction-aware physical geometry consumers;
- source-order and semantic-order preservation while visual geometry changes;
- shared layout, display-list, hit-test, overflow/scroll, and decoded-raster
  coordinates for rolled-back flex placement;
- no false unsupported-value diagnostics and no public declaration-keyword
  leakage.

Targeted checks follow the completed behavioral unit. Full native,
two-crate, strict, fuzz/security, and static documentation gates are final
validation only; remote CI remains unclaimed until an explicitly authorized
push.
