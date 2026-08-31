---
id: native-engine-055
scope: glass-browser/native-engine/inherited-text-transform
status: active
depends-on: [native-engine-054]
---

# Native bounded inherited text transform

## Objective

Add a bounded inherited `text-transform` presentation slice to the existing
fixed-cell text-flow owner. Supported local fixtures should be able to render
ASCII text in its authored, uppercase, or lowercase presentation form without
creating a second text or layout owner.

## Contract

The native CSS grammar accepts only `text-transform: none`, `uppercase`, and
`lowercase`. The property is inherited through the existing DOM style walk;
the initial value is `none`. Invalid values, `capitalize`, CSS-wide keywords,
locale-specific forms, and other transform syntax are diagnosed as unsupported
and do not change the cascade.

The layout pass applies the inherited transform to ASCII letters in each
fixed-cell text fragment before whitespace handling, wrapping, text-fragment
matching, display-list projection, and root-overflow measurement. ASCII case
conversion preserves whitespace, non-ASCII characters, and one-character
fixed-cell width; non-ASCII text is retained exactly. Semantic DOM text,
accessible names, compact evidence, and text locators retain authored source
text rather than the presentation form.

Direct text, nested inline text, `display:contents`, `<br>`, `pre`,
`pre-wrap`, and `nowrap` paths all use the same transformed fragment output.
Text transform does not change element boxes, line height, hit-test ownership,
scroll offsets, opacity groups, color, decoration, revisions, navigation
state, or action semantics. The fixed glyph rasterizer remains a bounded
case-insensitive ASCII subset; this slice does not implement Unicode case
mapping or font-specific lowercase glyph metrics.

No dependency, text cache, semantic-text mutation, locale handling, full
Unicode case folding, `capitalize`, `full-width`, `full-size-kana`, CSS-wide
values, font shaping, bidi, or browser text-rendering parity is introduced.

## Tradeoffs

- Applying the transform during layout keeps text fragments, wrapping,
  overflow, display-list commands, and text-fragment navigation aligned, but
  means layout carries a presentation string while semantic evidence retains
  source text.
- ASCII-only conversion guarantees fixed-cell width and avoids Unicode case
  expansion or locale policy, but leaves non-ASCII and language-sensitive
  casing unchanged.
- Reusing the existing fixed glyph table keeps the build and runtime surface
  small, but the raster output remains a canonical ASCII glyph subset rather
  than font-aware uppercase/lowercase shapes.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- parser, invalid-value diagnostics, selector/inline cascade, and inherited
  `none`/`uppercase`/`lowercase` values are covered by unit tests;
- direct, nested, `display:contents`, preformatted, wrapped, and nowrap text
  preserve source boundaries while carrying transformed layout fragments;
- text-fragment matching, display-list commands, root overflow, and decoded
  capture consume the same transformed fixed-cell output;
- authored semantic evidence and text locators remain source-text based;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass;
- the checkpoint is committed locally before issue #40 is updated and exact
  regenerable Cargo outputs are reclaimed.

## Completion evidence

To be filled after implementation and validation. Remote CI remains pending
until this local branch is pushed; no push, tag, publication, or release is
part of this epic checkpoint.
