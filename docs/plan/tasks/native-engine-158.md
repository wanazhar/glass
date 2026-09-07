---
id: native-engine-158
scope: glass-browser/native-engine/cascade-border-hidden-shorthand
status: planned
depends-on: [native-engine-157]
---

# Native bounded omitted-component `border:hidden`

## Objective

Accept the exact case-insensitive omitted-component `border:hidden` form and
the four physical `border-top:hidden`, `border-right:hidden`,
`border-bottom:hidden`, and `border-left:hidden` forms in the native border
owner. Preserve the existing complete `Npx style color` shorthand grammar,
the exact omitted-component `none` support from native-engine-157, and
standalone `revert-layer`.

The new form must enter the existing private hidden style stream so a winning
`hidden` blocks lower styles and cannot be resurrected by width or color
components. The private distinction must survive computed-style resolution so
future collapsed-table conflict work can assign it its own semantics, while
the current non-table engine continues to produce the existing no-side/
zero-width result.

## Context

The native engine already supports physical `border-style:hidden` and the
exact omitted-component `border:none` family. CSS also permits the border
shorthand to omit its other components when the value is exactly `hidden`; the
current parser rejects that syntax even though the private style owner already
has a distinct hidden sentinel. This slice closes that precise shorthand gap
without inventing the full CSS shorthand default machinery (`medium`,
`currentColor`, arbitrary style-only forms, or CSS-wide resets).

Normative reference:

- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthand>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-157.md`
- `docs/plan/tasks/native-engine-155.md`

## Contract

### Declaration and cascade state

- `border`, `border-top`, `border-right`, `border-bottom`, and `border-left`
  accept the existing complete bounded `Npx style color` form, the existing
  standalone case-insensitive `revert-layer` form, the exact
  case-insensitive token `none`, or the exact case-insensitive token `hidden`.
- The exact `hidden` form is represented by a private declaration-only value
  and feeds the existing per-side `NativeBorderStyleValue::Hidden` cascade
  stream. It does not add a public `NativeBorderStyle::Hidden` variant.
- A winning omitted-component `hidden` blocks lower style candidates and
  resolves to the existing no-side/zero-width result in the current non-table
  engine. Width and color candidates cannot resurrect that side. If a later
  style rollback exposes a lower painted style, existing lower component
  candidates remain eligible under the bounded native composition rule.
- The private hidden value remains distinguishable from `none` until the
  current computed-style boundary so future collapsed-table conflict
  resolution can assign hidden its required meaning. This slice does not add
  a table layout or border-conflict owner.
- `hidden` with additional tokens, other CSS-wide keywords, empty values,
  malformed complete values, style-only shorthand values other than exact
  `hidden`/`none`, dimensions without a complete value, and unsupported CSS
  syntax remain typed unsupported-value diagnostics. Raw CSS text is not added
  to diagnostics or public protocol output.
- Named-layer priority, specificity, source order, unlayered precedence,
  inline precedence, same-block declaration order, repeated rollback, and
  valid-before-invalid preservation remain authoritative. A shorthand
  `hidden` participates at its declaration order in all three existing
  component owners, with the private hidden style stream controlling current
  visibility.

### Existing owners preserved

- Complete border declarations retain their current private value and their
  independent width/style/color cascade projections. Omitted-component hidden
  declarations reuse the existing hidden style sentinel rather than changing
  display-list or raster command types.
- `NativeBorderSide`, `NativeBorder`, public `NativeBorderStyle`,
  `NativeBorderPaintSide`, display-list commands, rounded masks, clipping,
  opacity, viewport projection, capture, software raster, point-hit, and
  semantic/source-order schemas remain structurally unchanged.
- The slice remains fixture-relative, horizontal-tb, integer-pixel, and
  non-table. It does not add arbitrary omitted shorthand defaults, CSS-wide
  reset machinery, collapsed-table conflict resolution, logical sides,
  `currentColor`, gradients, border-image, animation, multiple origins,
  `!important` inversion, or browser-wide CSS border conformance.

## Tradeoffs

- Reusing the existing private hidden style sentinel keeps the public paint
  enum and artifact schemas stable, while preserving information needed by a
  future table-conflict owner.
- Exact-only `hidden` closes the high-value shorthand gap without silently
  claiming the default values or full grammar of every omitted-component
  border declaration.
- Omitted-component hidden declarations do not invent width or color
  candidates. This keeps the existing bounded component rollback rule
  explicit and avoids fake geometry or paint values.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- exact case-insensitive `hidden` parsing for the complete and four physical
  border shorthand properties, existing complete values, exact `none`,
  standalone `revert-layer`, and typed rejection of mixed, CSS-wide,
  malformed, style-only, and unsupported inputs;
- private declaration separation, named-layer/specificity/source-order/
  inline precedence, same-block order, repeated rollback, valid-before-invalid
  preservation, and independent width/color interaction;
- hidden-versus-none private separation through current resolution, no-side
  geometry, absent border commands for the current non-table path, decoded
  raster, clipping, point-hit testing, capture, semantic/source order, and
  unchanged public schemas; and
- focused `glass-browser` check/tests, full native integration/library tests,
  strict affected-package Clippy, rustdoc, paired-crate check/build,
  formatting, documentation, and final static gates. Remote CI remains
  unclaimed until an explicitly authorized push.

## Implementation

To be filled after the implementation checkpoint.

## Evidence

To be filled after local certification and bounded target cleanup.
