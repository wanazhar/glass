---
id: native-engine-059
scope: glass-browser/native-engine/inherited-font-weight
status: active
depends-on: [native-engine-058]
---

# Native bounded inherited font weight

## Objective

Add a bounded inherited `font-weight` presentation slice to the existing
fixed-cell text-flow and software-raster owners. Supported local fixtures
should be able to request a deterministic normal or bold text treatment while
keeping layout, semantics, hit testing, display-list projection, and capture
on the existing coordinate path.

## Contract

The native CSS grammar accepts only `normal`, `bold`, `400`, and `700` for
`font-weight`. `normal` and `400` resolve to `Normal`; `bold` and `700` resolve
to `Bold`. The property is inherited through the existing DOM style walk and
has `Normal` as its initial value. Other numeric weights, relative keywords,
CSS-wide keywords, and malformed values are diagnosed as unsupported and do
not change the computed value.

`Normal` retains the existing fixed 5x7 ASCII glyph replay. `Bold` performs a
bounded one-pixel horizontal dilation of each set glyph pixel, replaying the
original pixel and its immediate right neighbor. The fixed six-cell glyph
advance, letter spacing, word spacing, line height, wrapping, and all document
geometry remain unchanged. The dilation is clipped by the existing viewport
and ancestor rectangles; it never paints outside the same logical raster
surface or creates a second text/layout owner.

The resolved weight is carried on immutable text display commands. Raster
replay consumes that command value rather than re-reading mutable document
style, so direct display-list inspection, software painting, opacity groups,
scroll projection, and PNG capture observe the same presentation choice.
Semantic source text, accessible text, text-fragment matching, revisions,
locators, controls, text origins, measured overflow, and point hit testing
remain unchanged because the bounded weight does not alter advance metrics.

This is a deterministic native fixture rule, not font-weight or browser text
rendering parity. It does not implement font selection, font loading, font
metrics, real font faces, variable-font axes, numeric interpolation,
synthetic-bold policy, Unicode shaping, grapheme clusters, bidi, anti-aliasing,
or platform-specific glyph behavior. Unsupported non-ASCII glyph fallback
remains unchanged.

## Tradeoffs

- A one-pixel dilation makes bold text observable without importing a font
  dependency or changing the established integer-cell geometry, but it is a
  raster approximation and cannot reproduce real font weights.
- Keeping the advance and all layout coordinates unchanged preserves the
  shared hit-test/overflow contract and makes the feature cheap to reason
  about, but unusually dense glyphs may look heavier without gaining layout
  width.
- Carrying the resolved value through immutable commands keeps replay stable
  across scroll, opacity, and capture, but adds one presentation field to the
  experimental display/raster contract.
- Accepting only two normalizable pairs keeps diagnostics and build scope
  bounded, but common intermediate numeric weights and font-family-dependent
  behavior remain unsupported and visible.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- parser, initial value, stylesheet/inline cascade, inheritance, child
  override, and invalid-value diagnostics are covered by unit tests;
- normal and bold text retain identical layout rectangles, origins, advance,
  wrapping, text fragments, root overflow, and hit-test results;
- display-list commands carry the resolved normal/bold value and preserve it
  through opacity groups, scrolling, and capture;
- bold replay paints the deterministic one-pixel dilation, while normal replay
  remains byte-compatible with the pre-slice glyph path;
- clipping, alpha compositing, underline, letter spacing, and word spacing
  compose through the existing raster owners;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass;
- the checkpoint is committed locally before issue #40 is updated and exact
  regenerable Cargo outputs are reclaimed.

## Completion evidence

This design checkpoint is active. Implementation and validation evidence will
be added here before the task is marked complete.
