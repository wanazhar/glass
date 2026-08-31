---
id: native-engine-054
scope: glass-browser/native-engine/fixed-cell-text-decoration
status: active
depends-on: [native-engine-053]
---

# Native bounded fixed-cell text decoration

## Objective

Add one useful text-paint decoration to the existing fixed-cell native
renderer. Supported text should be able to opt into an inherited one-pixel
underline while preserving the current layout, display-list ownership,
software replay, hit testing, and scrolling contracts.

## Contract

The native CSS grammar accepts `text-decoration: none` and
`text-decoration: underline`. The bounded value is inherited through the
existing DOM style walk; the initial value is `none`, and an explicit child
`none` clears the bounded inherited decoration. Values containing multiple
lines or unsupported lines such as `overline`, `line-through`, or `blink`, as
well as decoration colors, styles, thicknesses, offsets, and CSS-wide values,
are diagnosed as unsupported and do not change the cascade.

Each fixed-cell text display command carries the computed underline bit. The
software rasterizer draws one logical pixel at the fixed glyph baseline
offset, spanning the bounded fixed-cell run width, using the run's existing
RGBA color and clip. Line fragments underline independently; empty runs do
not emit paint. The decoration is available for direct text and descendant
text nodes, including content flowing through `display:contents`.

Text decoration does not change text width, line breaking, line height, box
dimensions, root overflow, point hit testing, actionability, scrolling,
opacity-group boundaries, capture, revisions, or navigation. It does not
implement font metrics, descender-aware positioning, shaping, bidi, text
decoration propagation parity, decoration skip behavior, anti-aliasing,
physical pixels, or arbitrary font/text decoration support. No dependency or
second text/layout owner is introduced.

## Tradeoffs

- A fixed baseline offset keeps the result deterministic and dependency-free,
  but it is not a font-aware underline position and may not match browser
  descender or font metrics.
- Carrying the bit in the immutable text command keeps raster replay
  revision-stable and avoids re-reading mutable style during painting, while
  intentionally limiting the command to one decoration variant.
- Treating the bounded value as inherited makes nested fixture behavior easy
  to inspect, but differs from the full CSS text-decoration propagation
  model; that limitation remains visible in diagnostics and documentation.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- parsing, invalid-value diagnostics, selector/inline cascade, and inherited
  `none`/`underline` values are covered by unit tests;
- direct text, nested text, `display:contents`, explicit clearing, and
  multiline fixed-cell fragments are covered by integration tests;
- the display-list command carries the expected decoration and the surface
  contains a clipped, alpha-aware underline at the deterministic baseline;
- existing color, opacity, clipping, scrolling, capture, layout, hit-test,
  navigation, and revision behavior remains green;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass;
- the checkpoint is committed locally before issue #40 is updated and exact
  regenerable Cargo outputs are reclaimed.

## Completion evidence

Implementation is not yet complete. This task records the contract before
source changes; the next checkpoint must replace this section with exact
local test, lint, documentation, commit, issue, and cleanup evidence.
