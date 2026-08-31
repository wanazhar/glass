---
id: native-engine-056
scope: glass-browser/native-engine/first-line-text-indent
status: active
depends-on: [native-engine-055]
---

# Native bounded first-line text indent

## Objective

Add a bounded `text-indent` presentation slice to the existing fixed-cell
block-flow owner. Supported local fixtures should be able to shift the first
line of a block container while keeping later lines, child block boxes, paint,
hit testing, and semantic evidence on the existing shared layout path.

## Contract

The native CSS grammar accepts only non-negative fixed-pixel values for
`text-indent`, including `0px`, bounded by the existing native dimension limit.
Negative lengths, percentages, unitless values, keywords, CSS-wide keywords,
and other syntax are diagnosed as unsupported and do not change the computed
value. The initial value is `0px`; the property is not inherited.

For a block container, the computed indent shifts the first line's fixed-cell
content origin and reduces that line's available width before whitespace
handling, wrapping, text-fragment matching, display-list projection, and
root-overflow measurement. To keep the bounded flow owner able to place text,
the effective indent is clamped so the first line retains at least one fixed
character cell; later lines use the full content width. Hard breaks, `pre`,
`pre-wrap`, and `nowrap` consume the same first-line state. Inline and
`display:contents` elements do not create an independent indent context; their
text uses the containing block's flow.

The indent changes presentation coordinates and line capacity only. It does not
change block box geometry, line height, source text, accessible names, compact
evidence, locators, revisions, navigation, action semantics, opacity, color,
decoration, or root scroll offsets except where the shared measured text extent
requires a larger bounded overflow width. The existing text alignment offsets,
fixed-cell glyph table, and raster path remain the owners of their respective
concerns.

No negative/hanging indentation, `each-line`, `hanging`, percentage or
font-relative units, bidi/logical writing modes, inline-formatting parity,
font metrics, or browser CSS conformance is introduced.

## Tradeoffs

- Applying indentation in `FlowCursor` preserves one coordinate source for
  wrapping, text fragments, paint, hit testing, and overflow, but makes the
  first-line capacity intentionally narrower than subsequent lines.
- Clamping to one fixed cell keeps the bounded engine productive for oversized
  authored indents, but does not model CSS's unrestricted overflow behavior.
- Keeping the value non-inherited and block-only matches the useful ownership
  boundary without adding a second inline formatting context, but leaves
  complex nested/inline CSS semantics outside the slice.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- parser, invalid-value diagnostics, cascade, initial value, and non-inherited
  computed behavior are covered by unit tests;
- block first-line placement, reduced first-line wrapping capacity, later-line
  reset, hard breaks, preformatted flow, and nowrap flow are covered by native
  integration fixtures;
- inline and `display:contents` ownership stays on the containing block flow;
- text fragments, display-list commands, hit testing, and root overflow use the
  same indented layout coordinates while semantic source evidence stays stable;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass;
- the checkpoint is committed locally before issue #40 is updated and exact
  regenerable Cargo outputs are reclaimed.

## Completion evidence

To be filled after implementation and validation. Remote CI remains pending
until this local branch is pushed; no push, tag, publication, or release is
part of this epic checkpoint.
