---
id: native-engine-157
scope: glass-browser/native-engine/cascade-border-none-shorthand
status: planned
depends-on: [native-engine-156]
---

# Native bounded omitted-component `border:none`

## Objective

Accept the exact case-insensitive omitted-component `border:none` form and
the four physical `border-top:none`, `border-right:none`, `border-bottom:none`,
and `border-left:none` forms in the native border owner. Preserve the existing
complete `Npx style color` shorthand grammar and standalone `revert-layer`.
The new form must enter the existing private no-paint style stream so a
winning `none` blocks lower styles and cannot be resurrected by width or color
components.

## Context

The native engine already supports physical `border-style:none` and the
complete-value `border`/physical border shorthands. CSS also permits a
border shorthand to omit its other components when the value is exactly
`none`; the current parser rejects that syntax, leaving a stale product claim
that the bounded no-paint style is available only through `border-style`.
This slice adds the smallest useful omitted-component form without inventing
the full CSS shorthand default machinery (`medium`, `currentColor`, arbitrary
style-only forms, or CSS-wide resets).

Normative reference:

- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthand>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-156.md`
- `docs/plan/tasks/native-engine-154.md`

## Contract

### Declaration and cascade state

- `border`, `border-top`, `border-right`, `border-bottom`, and `border-left`
  accept the existing complete bounded `Npx style color` form, the existing
  standalone case-insensitive `revert-layer` form, or the exact
  case-insensitive token `none`.
- The exact `none` form is represented by a private declaration-only
  no-paint value and feeds the existing per-side `NativeBorderStyleValue::None`
  cascade stream. It does not add a public `NativeBorderStyle::None` variant.
- A winning omitted-component `none` blocks lower style candidates and
  resolves to the existing no-side/zero-width result. Width and color
  candidates cannot resurrect that side. If a later style rollback exposes a
  lower painted style, existing lower component candidates remain eligible;
  this is the bounded native component-composition rule, not a claim of full
  CSS shorthand-default parity.
- `none` with additional tokens, other CSS-wide keywords, empty values,
  malformed complete values, `hidden` shorthand, style-only shorthand forms,
  dimensions without a complete value, and unsupported CSS syntax remain typed
  unsupported-value diagnostics. Raw CSS text is not added to diagnostics or
  public protocol output.
- Named-layer priority, specificity, source order, unlayered precedence,
  inline precedence, same-block declaration order, repeated rollback, and
  valid-before-invalid preservation remain authoritative. A shorthand `none`
  participates at its declaration order in all three existing component
  owners, with the no-paint style stream controlling current visibility.

### Existing owners preserved

- Complete border declarations retain their current private value and their
  independent width/style/color cascade projections. Physical no-paint
  shorthand declarations reuse the existing style sentinel rather than
  changing display-list or raster command types.
- `NativeBorderSide`, `NativeBorder`, public `NativeBorderStyle`,
  `NativeBorderPaintSide`, display-list commands, rounded masks, clipping,
  opacity, viewport projection, capture, software raster, point-hit, and
  semantic/source-order schemas remain structurally unchanged.
- The slice remains fixture-relative, horizontal-tb, integer-pixel, and
  non-table. It does not add `border:hidden` shorthand, arbitrary omitted
  shorthand defaults, CSS-wide reset machinery, collapsed-table conflict
  resolution, logical sides, `currentColor`, gradients, border-image,
  animation, multiple origins, `!important` inversion, or browser-wide CSS
  border conformance.

## Tradeoffs

- A private declaration wrapper is a small cascade-shape change, but it keeps
  complete borders and no-paint shorthand values typed instead of encoding
  `none` through a fake width, color, or public enum value.
- The slice accepts only exact `none`; this closes the high-value no-paint
  syntax gap without silently claiming `medium`, `currentColor`, or every
  omitted-component combination.
- No-paint shorthand declarations do not invent width or color candidates.
  This preserves existing lower-component rollback behavior and keeps the
  current engine’s explicit style-stream ownership visible to reviewers.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- exact case-insensitive `none` parsing for the complete and four physical
  border shorthand properties, existing complete values, standalone
  `revert-layer`, and typed rejection of mixed, CSS-wide, malformed,
  `hidden`, and unsupported inputs;
- private declaration separation, named-layer/specificity/source-order/
  inline precedence, same-block order, repeated rollback, valid-before-invalid
  preservation, and independent width/color interaction;
- no-side geometry, absent border commands, decoded raster, clipping,
  point-hit testing, capture, semantic/source order, and unchanged public
  schemas; and
- focused `glass-browser` check/tests, full native integration/library tests,
  strict affected-package Clippy, rustdoc, paired-crate check/build,
  formatting, documentation, and final static gates. Remote CI remains
  unclaimed until an explicitly authorized push.

## Implementation

To be filled after the implementation checkpoint.

## Evidence

To be filled after local certification and bounded target cleanup.
