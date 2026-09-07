---
id: native-engine-162
scope: glass-browser/native-engine/cascade-border-complete-current-color
status: planned
depends-on: [native-engine-161]
---

# Native bounded complete border `currentColor`

## Objective

Accept the exact case-insensitive `currentColor` keyword as the color
component of the existing complete physical border shorthand forms:
`border`, `border-top`, `border-right`, `border-bottom`, and `border-left`.
The accepted bounded forms are `Npx <painted-style> currentColor`,
`Npx none currentColor`, and `Npx hidden currentColor`. Preserve the existing
literal-color complete forms, omitted-component `none`/`hidden`, independent
width/style/color streams, and concrete public `NativeBorder` values.

This slice completes the deferred-color path for complete physical shorthands.
It does not add omitted width/style defaults, logical sides, CSS-wide reset
machinery, or general CSS color syntax.

## Context

`native-engine-161` added private `NativeBorderColorValue::CurrentColor` to
standalone physical `border-color` and its four physical color longhands. The
complete border parser still requires `parse_color`, so
`border: 2px solid currentColor` and the corresponding complete `none` and
`hidden` forms are rejected. The existing complete declaration wrapper already
projects width, style, and concrete color independently; this slice adds
deferred-color complete variants that reuse the same component projections.

Normative references:

- <https://www.w3.org/TR/css-color-4/#currentcolor-color>
- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthand>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-161.md`
- `docs/plan/tasks/native-engine-160.md`

## Contract

### Declaration and cascade state

- Each of the five physical complete border shorthand names accepts the
  existing bounded `Npx <style> <literal-color>` form plus exact,
  case-insensitive `Npx <style> currentColor` when `<style>` is one of the
  existing painted styles, `none`, or `hidden`.
- Complete deferred-color declarations carry bounded width and, where
  applicable, style privately. They project `CurrentColor` into the existing
  independent border color stream at the same declaration order. A winning
  `none` or `hidden` still suppresses current non-table paint and preserves its
  existing private style sentinel; deferred width/color must not resurrect a
  no-paint side.
- Standalone `revert-layer`, omitted-component `none`/`hidden`, concrete
  complete values, physical color longhands, layer priority, specificity,
  source order, unlayered/inline precedence, same-block declaration order,
  repeated rollback, and valid-before-invalid preservation remain
  authoritative.
- Missing or extra components, malformed dimensions/colors, unsupported style
  tokens, other CSS-wide keywords, arbitrary omitted defaults, and unsupported
  color functions remain typed unsupported-value diagnostics. Complete
  `currentColor` does not change the bounded parser's rejection of unrelated
  syntax.

### Existing owners preserved

- `NativeBorderColorValue::CurrentColor` remains private declaration/cascade
  state until computed-style resolution. Public `NativeBorderSide` and
  `NativeBorder` continue to contain concrete `NativeColor`; no public enum,
  diagnostic payload, display-list field, raster schema, dependency, feature
  default, or crate boundary changes.
- Resolved complete borders continue through existing width/style composition,
  physical box insets, display-list commands, rounded masks, clipping, opacity,
  viewport projection, capture, software raster, point-hit, and
  semantic/source-order projection.
- The slice remains fixture-relative, horizontal-tb, integer-pixel, non-table,
  and feature-gated behind `native-engine`. It does not implement omitted
  `medium`/`currentColor` inference, logical border sides, gradients, system
  colors, color spaces, percentages, animations, multiple origins,
  `!important` inversion, collapsed-table conflict resolution, or browser-wide
  CSS color/border conformance.

## Tradeoffs

- Separate private complete deferred variants preserve the existing public
  `NativeBorderSide` shape and keep no-paint `none`/`hidden` state distinct,
  while reusing the color stream and computed-color substitution proven in
  native-engine-161.
- Supporting all three current complete style families in one bounded slice
  avoids a syntax hole where painted, none, and hidden forms disagree, but it
  intentionally does not infer any omitted component defaults.
- Complete shorthand resolution still happens at the element computed-style
  boundary, so local/inherited `color` is read once and current-color values
  cannot leak into paint-time or public protocol state.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive complete painted, `none`, and `hidden` `currentColor`
  parsing for all five physical shorthand names, concrete form preservation,
  omitted forms, standalone `revert-layer`, and typed rejection of malformed,
  incomplete, CSS-wide, unsupported, and unrelated forms;
- local, inherited, and bounded-black color substitution; independent
  width/style/color projections; layer/specificity/source-order/inline
  precedence; same-block order; repeated rollback; valid-before-invalid
  preservation; and private/public separation;
- current no-paint behavior for complete `none`/`hidden`, painted border
  geometry, display-list color/style, decoded raster, clipping, capture,
  point-hit, semantic/source order, and diagnostic behavior; and
- focused feature check/tests, full native integration/library tests, strict
  affected-package Clippy, rustdoc, paired-crate check/build, formatting,
  documentation, final static gates, and bounded regenerable-target cleanup.
  Remote CI remains unclaimed until an explicitly authorized push.

## Implementation

Pending. The design is recorded before implementation so complete physical
border shorthand `currentColor` cannot be accepted without explicit coverage
of painted and private no-paint variants.

