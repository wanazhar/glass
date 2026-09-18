# Native-engine browser slice 519: variation-aware glyph rasterization

Status: complete locally on the current source line.

## Objective

Make non-default variable-font coordinates affect the native glyph bitmap as
well as HarfRust shaping. A shaped run must not pair variation-aware advances
with a default-instance outline when the selected face has a usable TrueType
or CFF outline.

## Contract

- Keep the existing `fontdue` rasterizer for ordinary static/default-instance
  glyphs and as a bounded fallback for bitmap-only or malformed variable
  glyphs.
- Parse the selected face with `ttf-parser`'s variable-font support, apply the
  already-admitted bounded axis coordinates, obtain the variation-aware
  outline, flatten lines/quadratics/cubics under a fixed point budget, and
  produce an antialiased coverage bitmap in the same baseline coordinate
  contract as fontdue.
- Reuse the existing horizontal synthetic stretch step and HarfRust glyph
  placement; the rasterizer owns only the glyph-local outline and metrics.
- Keep raster work bounded by the existing font-size limit plus fixed outline,
  bitmap, and supersampling limits. Malformed or unsupported outlines must
  fail closed to the existing fontdue path rather than panic or cross a new
  capability boundary.
- Cover both shaped glyphs and the character-by-character fallback path.

## Tradeoffs and explicit boundary

The implementation adds no raster dependency and pays the extra outline and
supersampling cost only for a non-empty variation request. The bounded
software rasterizer is intentionally simpler than platform text rasterization;
hinting, color glyph layers/bitmap tables, automatic `font-weight` and
`font-stretch` axis mapping, WOFF2, and complete FontFace/Web IDL parity remain
separate issue #40 gates.

## Implementation path

1. Enable the already-used `ttf-parser` variable-font feature.
2. Add a bounded outline collector and coverage rasterizer in the native font
   owner.
3. Select the variation-aware raster path for both glyph consumers while
   preserving fontdue fallback behavior.
4. Add an Ubuntu variable-font witness that proves coverage changes, not only
   shaped width, at a non-default axis value.
5. Update the architecture, analysis, plan, and issue checkpoint after local
   validation.

## Verification

The final record will include formatting/diff checks, a scoped package check,
focused font tests, the affected `font_` group, the full locked
`glass-browser` library regression, and the repository documentation
validators. No remote CI, push, release, tag, registry publication, or
native/CDP parity certification is implied by this local slice.

## Local results

- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo check --quiet -p glass-browser --lib --locked` passed in 52.27
  seconds after enabling `ttf-parser` variable-font support.
- The focused variable-font shaping and bitmap witness passed `1/1` in 7.81
  seconds; the affected `font_` group passed `85/85` in 28.47 seconds.
- The full locked `glass-browser` library regression passed `1,220` tests with
  1 ignored and 0 failures in 63.93 seconds.
- The Ubuntu variable-font witness now verifies that a non-default `wdth`
  coordinate changes both shaped advance and glyph coverage. The existing
  fontdue path remains the fallback for static, bitmap-only, and malformed
  variable outlines.
