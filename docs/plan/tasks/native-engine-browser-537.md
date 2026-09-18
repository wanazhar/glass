# Native-engine browser slice 537: COLR palette plumbing

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Remove the hard-coded COLRv1 palette argument from the native color-glyph
raster path while preserving bounded admission and fail-closed behavior for
invalid palette selections.

## Scope

- Carry a bounded `u16` palette index with native text metrics into COLR/CPAL
  glyph painting.
- Validate the selected index against the face's admitted CPAL palette count
  before invoking `ttf-parser` color painting.
- Preserve palette zero as the current default until the CSS `font-palette`
  and `@font-palette-values` surfaces are wired in a later slice; no CDP or
  remote-browser path is introduced.

## Contract

- Palette selection changes COLR/CPAL paint colors only; shaping, metrics,
  variation coordinates, and monochrome fallback remain unchanged.
- Palette indices are face-local and bounded by `Face::color_palettes()`;
  out-of-range indices fail closed to the existing monochrome path.
- The raster path retains the existing glyph, stop, transform, clip, surface,
  and supersampling limits.

## Verification

- The non-default palette and out-of-range rejection regression passed 1/1.
- The focused COLRv1 unit group passed 8/8.
- The serialized locked `glass-browser` library regression passed 1,253
  tests with 1 ignored. Two existing FontFace local-system tests remain
  sensitive to parallel test execution; each passed in isolation.
- The native integration gate
  `native_real_font_metrics_feed_layout_and_glyph_paint_when_available`
  passed 1/1.
- Both package checks, the `glass` debug build, locked metadata, formatting,
  and all three documentation validators passed.
- No CSS `font-palette`/`@font-palette-values`, all-target, cross-platform,
  release, remote CI, push, or issue-closure claim is made by this checkpoint.
