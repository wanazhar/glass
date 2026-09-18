# Native-engine browser slice 538: CSS font-palette keywords

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Expose the inherited CSS `font-palette` keyword path for native COLR/CPAL
rasterization without changing shaping, metrics, font selection, or fallback.

## Scope

- Parse and cascade the CSS `font-palette` keywords `normal`, `light`, and
  `dark`.
- Resolve `light` and `dark` against bounded CPAL palette flags from the
  selected face; use palette zero when the requested semantic palette is not
  advertised.
- Preserve palette zero for `normal` and for all monochrome/fallback paths.
- Keep custom palette names and `@font-palette-values` outside this slice.
- Keep the native-only production path; no CDP fallback or compatibility shim.

## Contract

- `font-palette` is inherited and CSS-wide `inherit`, `initial`, `unset`, and
  `revert`/`revert-layer` behavior follows the existing text-property cascade.
- Palette selection affects only COLR/CPAL paint colors. Font matching, shaping,
  metrics, variation coordinates, and monochrome fallback remain unchanged.
- CPAL palette metadata is read from the selected face and bounded by the face's
  palette count; malformed or unavailable metadata fails closed to palette zero.
- The existing native raster, transform, clip, surface, and supersampling limits
  remain unchanged.

## Verification

- The CPAL semantic palette resolver passed 1/1, selecting fixture flags
  `light -> 2` and `dark -> 1` while malformed input fell back to zero.
- The text-metrics raster regression passed 1/1 with a non-default palette
  changing COLR layer colors.
- CSS parser, inherited cascade, and CSSOM computed-style regressions passed
  1/1 each.
- The serialized locked `glass-browser` library suite passed 1,258 tests with
  1 ignored.
- The native integration gate
  `native_real_font_metrics_feed_layout_and_glyph_paint_when_available`
  passed 1/1.
- Package checks, the `glass` debug build, locked metadata, formatting, and
  all three documentation validators passed.
- No custom palette names, `@font-palette-values`, all-target,
  cross-platform, release, remote CI, push, or issue-closure claim is made by
  this checkpoint.
