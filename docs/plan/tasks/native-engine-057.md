---
id: native-engine-057
scope: glass-browser/native-engine/inherited-word-spacing
status: active
depends-on: [native-engine-056]
---

# Native bounded inherited word spacing

## Objective

Add a bounded inherited `word-spacing` presentation slice to the existing
fixed-cell text-flow owner. Supported local fixtures should be able to widen
rendered word separators while keeping wrapping, alignment, text fragments,
display-list output, raster replay, hit testing, and root overflow on one
shared coordinate path.

## Contract

The native CSS grammar accepts only non-negative fixed-pixel values for
`word-spacing`, including `0px`, bounded by the existing native dimension
limit. The property is inherited through the existing DOM style walk; its
initial value is `0px`. Negative lengths, percentages, unitless values,
font-relative units, keywords, CSS-wide keywords, and other syntax are
diagnosed as unsupported and do not change the computed value.

The spacing value adds a fixed advance after each rendered ASCII space. In
`normal`, `nowrap`, and `pre-line`, the existing whitespace-collapse path
continues to emit at most one separator space and that separator receives the
extra advance. In `pre` and `pre-wrap`, each literal ASCII space in a
non-line-break segment receives the extra advance; LF, CR, and CRLF boundaries,
tabs, and other whitespace keep their existing bounded fixed-cell behavior.
The value does not add a space to text, change source text, or alter hard line
break ownership. `text-indent` remains the first-line origin owner and
`text-align` consumes the resulting measured line width.

The measured extra advance is applied before soft wrapping, preformatted
chunking, text-fragment coordinates, text alignment, display-list projection,
root-overflow measurement, and raster replay. Immutable text display commands
carry the computed spacing so the fixed glyph rasterizer inserts the same
bounded gap after ASCII spaces; underline spans include that measured run
width. Semantic DOM text, accessible names, compact evidence, locators,
revisions, navigation, action semantics, block geometry, line height, opacity,
color, decoration, and indentation remain unchanged except for shared text
coordinates and any resulting root horizontal extent.

Spacing uses saturating bounded arithmetic. The existing fixed-cell glyph
advance remains the base for every character, and the spacing value is an
additional presentation advance rather than a replacement width. No second
text/layout owner or dependency is introduced.

No negative word spacing, `letter-spacing`, word-break or line-break policy,
Unicode whitespace classification, browser tab-stop metrics, font metrics,
justification, bidi/logical writing modes, or browser CSS conformance is
introduced.

## Tradeoffs

- Applying spacing inside the existing flow cursor keeps wrapping, alignment,
  fragments, overflow, paint, and hit testing coherent, but makes each
  rendered separator consume more of the bounded line width.
- Carrying the value on immutable text commands keeps raster output stable and
  avoids consulting mutable style during replay, but expands the command shape
  for one bounded spacing variant.
- Supporting literal ASCII spaces in `pre`/`pre-wrap` preserves one predictable
  rule across all existing text modes, but intentionally leaves tabs, Unicode
  whitespace, and browser-specific word boundaries at the fixed-cell default.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- parser, invalid-value diagnostics, stylesheet/inline cascade, initial value,
  and inherited child override behavior are covered by unit tests;
- collapsed `normal`/`nowrap`/`pre-line` separators and literal `pre`/
  `pre-wrap` spaces use the same bounded spacing arithmetic;
- wrapping, preformatted chunking, hard-break reset, `text-indent`, and
  `text-align` preserve the documented ownership boundaries;
- text fragments, display-list spacing metadata, underline extents, raster
  glyph positions, hit testing, and root overflow share the measured result;
- semantic source text/evidence, locators, revisions, navigation, actions,
  block geometry, line height, opacity, color, and decoration remain green;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass;
- the checkpoint is committed locally before issue #40 is updated and exact
  regenerable Cargo outputs are reclaimed.

## Completion evidence

To be filled after implementation and validation. Remote CI remains pending
until this local branch is pushed; no push, tag, publication, or release is
part of this epic checkpoint.
