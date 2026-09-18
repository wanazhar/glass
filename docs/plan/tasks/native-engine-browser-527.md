# Native-engine browser slice 527: bounded COLRv1 transforms and compositing

Status: complete on the current source line.

## Objective

Extend the native software text renderer's solid COLR path beyond simple
palette layers. Preserve the existing HarfRust shaping and coverage-raster
contract while admitting the bounded affine transforms and compositing modes
used by common COLRv1 solid glyphs.

## Contract

- Carry finite COLR transforms through a bounded transform stack and apply
  them to outline points before supersampled rasterization.
- Carry `SourceOver` and `DestinationOver` layer modes from `ttf-parser`
  through native glyphs into the surface compositor.
- Bound transform and current-outline clip nesting at 16 levels and reject
  non-finite or over-large transform components before raster admission.
- Treat a current-outline clip as balanced paint state: the accepted solid
  paint is the same outline that established the clip, so no additional mask
  is needed. Clip boxes remain rejected rather than silently ignored.
- Preserve glyph advance, shaping, spacing, fallback selection, hit testing,
  display-list ownership, WOFF/WOFF2 normalization, wire shape, two-crate
  boundary, and explicit CDP migration backend.
- Fall back to the existing monochrome outline path when the color paint uses
  gradients, clip boxes, unsupported composite modes, malformed nesting, or
  any boundedness failure.

## Tradeoffs and boundary

This slice covers the common solid-transform/composite COLRv1 forms without
adding a graphics dependency or changing the display-list protocol. It does
not claim gradient interpolation, clip-box masks, the remaining Porter-Duff
and blend modes, CBDT/CBLC or SVG-in-font decoding, palette selection APIs,
color variation-axis support, hinting, `font-display` timing, or complete
FontFace/Web IDL parity. Unsupported color data remains recoverable through
the monochrome path.

## Verification

- `cargo fmt --all` and `git diff --check` passed.
- Locked scoped `glass-browser` library check passed with plain diagnostics in
  0.26 seconds from the retained build graph.
- The color-specific font batch passed `4/4` in 0.03 seconds after the locked
  build; it covers two solid palette layers, unsupported gradient fallback,
  affine transforms, and solid composite layers.
- The serial native-engine unit batch passed `440/440` in 40.64 seconds with
  `RUST_MIN_STACK=8388608`, including raster compositing and all prior native
  engine tests.
- No remote CI, push, release, tag, registry publication, or native/CDP
  parity certification is implied by this local slice.

## Touched implementation

- `crates/glass-browser/src/browser/native_engine/font.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
