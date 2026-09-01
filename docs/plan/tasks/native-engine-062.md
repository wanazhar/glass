---
id: native-engine-062
scope: glass-browser/native-engine/text-overflow
status: active
depends-on: [native-engine-061]
---

# Native bounded text overflow

## Objective

Add a bounded local `text-overflow` presentation choice to the existing
single-line fixed-cell flow. Explicit fixture blocks should be able to keep
the current clipped text or replace the overflowing suffix with a deterministic
ASCII ellipsis marker without adding a second inline formatter, changing
semantic source text, or adding a renderer dependency.

## Contract

The native CSS grammar accepts only `clip` and `ellipsis` for
`text-overflow`. The property is not inherited; its initial computed value is
`Clip`. Stylesheet and inline declarations use the existing local cascade.
CSS-wide keywords, `fade`, malformed values, and other text-overflow syntax are
diagnosed as unsupported and do not replace the computed value.

The bounded behavior applies only when all of these conditions hold:

- the owner is a rendered block container;
- the owner uses inherited `white-space: nowrap`;
- the owner has an axis-specific horizontal `overflow-x: hidden|clip` clip;
- the owner has one direct text child and no inline descendants, `display:contents`
  subtree, or hard-break element; and
- the content width is positive and finite.

`clip` retains the existing full collapsed fixed-cell text run and lets the
shared overflow clip hide its out-of-box pixels. When the transformed,
collapsed text fits the content width, `ellipsis` is identical to `clip`. When
it does not fit, `ellipsis` emits one visual run containing the longest
spacing-aware prefix that fits before an ASCII `...` marker. The marker itself
uses the existing fixed-cell character advance and the same text presentation
(color, decoration, weight, style, word spacing, and letter spacing). The
marker is emitted only when all three cells fit; if the content width is too
narrow for it, the run falls back to the longest clipped prefix that fits.

The visual run is marked truncated so root overflow measurement and text
fragment targeting do not treat the generated presentation as complete. The
semantic DOM, `visible_text`, accessible name/source text, revisions,
locators, and action targets retain the full authored text. Box geometry,
line-height, line origin, hit-test ownership, paint order, scroll projection,
and capture remain on the existing owners; the visual text run may have a
shorter string and a different measured width.

`text-overflow` is intentionally a single-line fixture rule. It does not add
multi-line ellipsis, `line-clamp`, generated-content accessibility semantics,
RTL/vertical writing behavior, Unicode U+2026 marker selection, bidi,
grapheme-aware truncation, font metrics, shaping, or browser conformance.
Overflow without the required horizontal clip remains unchanged, as do
multiple direct text nodes and inline descendants. The existing `pre`,
`pre-wrap`, `normal`, and `pre-line` paths do not acquire text-overflow
behavior in this slice.

## Tradeoffs

- A fixed ASCII marker keeps the raster path deterministic and available in the
  existing glyph subset, but it is not the browser's locale/font-dependent
  ellipsis glyph.
- Restricting ellipsis to one direct text child avoids inventing cross-node
  whitespace and inline formatting rules, but common nested markup must remain
  on the established clip path.
- Reusing the existing overflow clip and immutable text command preserves
  paint, scroll, hit-test, and capture ownership, but visual truncation is
  exposed as a shortened layout run and cannot be used for exact text-fragment
  matching.
- Requiring `nowrap` and horizontal clipping makes the trigger deterministic,
  but multi-line and visible-overflow CSS behavior remain outside the claim.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- parser, initial value, local stylesheet/inline cascade, non-inheritance, and
  invalid-value diagnostics are covered by unit tests;
- clipped nowrap text retains the existing full visual run under `clip`, while
  overflowing eligible text under `ellipsis` emits a spacing-aware prefix and
  ASCII marker within the content width;
- fitting text, too-narrow marker space, leading/trailing collapsed whitespace,
  transformations, word/letter spacing, decoration, bold/italic, and ancestor
  clipping remain internally consistent;
- nested inline content, multiple text children, non-nowrap whitespace modes,
  and visible/non-horizontal overflow retain their existing behavior;
- semantic full text remains unchanged while visual truncated runs do not add
  root horizontal overflow or become text-fragment targets;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass;
- the checkpoint is committed locally before issue #40 is updated and exact
  regenerable Cargo outputs are reclaimed.

## Completion evidence

This design checkpoint is active. Implementation and validation evidence will
be added here before the task is marked complete.
