---
id: native-engine-159
scope: glass-browser/native-engine/cascade-border-hidden-complete
status: planned
depends-on: [native-engine-158]
---

# Native bounded complete `border: Npx hidden color`

## Objective

Accept the bounded complete-value `Npx hidden color` form for `border`,
`border-top`, `border-right`, `border-bottom`, and `border-left`, with
case-insensitive `hidden`. Preserve the exact omitted-component `none` and
`hidden` forms from native-engine-157/158, the complete painted `Npx style
color` grammar, and standalone `revert-layer`.

The complete hidden form must carry its declared width and color into the
existing independent component streams while mapping only its style to the
private `NativeBorderStyleValue::Hidden` sentinel. A winning hidden style must
still suppress current non-table paint, but its width/color state must remain
available to the bounded computed-style owner and future collapsed-table
conflict work. No public `Hidden` paint variant or new display-list schema is
permitted.

## Context

The native engine now accepts exact omitted-component `border:hidden` and the
four physical omitted-component forms, but its complete parser accepts only
the painted public styles. CSS also permits `hidden` as the style component of
a complete border value, for example `2px hidden red`. The current parser
rejects that syntax because `NativeBorderSide` intentionally stores only
public painted styles. This slice adds a private complete-hidden value rather
than leaking the table-sensitive state into public computed values.

Normative reference:

- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthand>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-158.md`
- `docs/plan/tasks/native-engine-157.md`

## Contract

### Declaration and cascade state

- The five physical border shorthand properties accept the existing complete
  bounded `Npx style color` form for public painted styles, the exact
  case-insensitive omitted-component `none` or `hidden` token, the complete
  bounded `Npx hidden color` form, or standalone case-insensitive
  `revert-layer`.
- A complete hidden declaration is represented by a private declaration-only
  value carrying its bounded width and parsed color. Its style projection is
  `NativeBorderStyleValue::Hidden`; its width and color projections are the
  declared values at the same declaration order. It does not add a public
  `NativeBorderStyle::Hidden` variant.
- A winning complete hidden style blocks current non-table paint and resolves
  to the existing no-side/zero-width result. Width and color remain typed
  private candidates for the future collapsed-table owner, but they cannot
  resurrect a painted side in the current computed-style composition. A later
  bounded `revert-layer` can expose an existing lower painted component.
- Complete hidden values with missing width/color, extra tokens, unsupported
  styles, other CSS-wide keywords, empty values, malformed dimensions/colors,
  style-only values other than the exact omitted-component token, and
  unsupported CSS syntax remain typed unsupported-value diagnostics. Raw CSS
  text is not added to diagnostics or public protocol output.
- Named-layer priority, specificity, source order, unlayered precedence,
  inline precedence, same-block declaration order, repeated rollback, and
  valid-before-invalid preservation remain authoritative for all three
  component streams.

### Existing owners preserved

- Exact omitted-component `hidden` continues to use the 158 private hidden
  declaration and does not invent width/color candidates. Complete painted
  borders and exact omitted-component `none` remain unchanged.
- `NativeBorderSide`, `NativeBorder`, public `NativeBorderStyle`,
  `NativeBorderPaintSide`, display-list commands, rounded masks, clipping,
  opacity, viewport projection, capture, software raster, point-hit, and
  semantic/source-order schemas remain structurally unchanged.
- The slice remains fixture-relative, horizontal-tb, integer-pixel, and
  non-table. It does not add collapsed-table conflict resolution, logical
  sides, arbitrary omitted defaults, CSS-wide reset machinery, `currentColor`,
  gradients, border-image, animation, multiple origins, `!important`
  inversion, or browser-wide CSS border conformance.

## Tradeoffs

- A private complete-hidden value preserves declared width/color for future
  table conflict resolution without forcing a public enum expansion or
  pretending that current non-table rendering paints hidden borders.
- The parser remains exact and bounded: it adds `Npx hidden color`, not
  arbitrary omitted combinations, CSS-wide keyword semantics, or a general
  hidden-border layout model.
- Width/color candidates are retained internally even though current
  composition suppresses them behind hidden style. This is deliberate state
  ownership for future table work and must be covered by private/public
  separation tests.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- case-insensitive complete `Npx hidden color` parsing for the shorthand and
  all four physical properties, preservation of complete painted/omitted
  values, standalone `revert-layer`, and typed rejection of incomplete,
  mixed, malformed, CSS-wide, and unsupported inputs;
- private complete-hidden width/style/color separation, declaration order,
  named-layer/specificity/source-order/inline precedence, same-block order,
  repeated rollback, and valid-before-invalid preservation;
- current no-side/zero-width geometry and absent border commands/raster while
  declared width/color remain privately typed, plus clipping, point-hit,
  capture, semantic/source order, and unchanged public schemas; and
- focused `glass-browser` check/tests, full native integration/library tests,
  strict affected-package Clippy, rustdoc, paired-crate check/build,
  formatting, documentation, and final static gates. Remote CI remains
  unclaimed until an explicitly authorized push.

## Implementation

To be filled after the implementation checkpoint.

## Evidence

To be filled after local certification and bounded target cleanup.
