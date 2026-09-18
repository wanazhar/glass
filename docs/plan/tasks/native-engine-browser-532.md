# Native-engine browser slice 532: bounded bitmap glyph images

Status: complete on the current source line.

## Objective

Make bounded raster images stored in bitmap font tables available to the native
software text renderer without changing shaping, advances, or the display-list
wire boundary.

## Contract

- Use `ttf-parser::Face::glyph_raster_image` for the selected font face,
  preserving its collection index for TTC/OTC system faces.
- Admit finite strikes up to 1,024 pixels per dimension and 4 MiB of decoded
  RGBA/coverage storage. Scale the selected strike to the requested ppem while
  retaining its glyph offsets.
- Decode PNG, premultiplied BGRA32, monochrome, and 2/4/8-bit grayscale image
  formats. Convert color strikes to straight RGBA plus embedded alpha; use
  grayscale strikes as coverage over the existing text paint.
- Carry optional per-pixel RGBA through the existing `NativeGlyph` and software
  compositor paths. Bitmap alpha takes precedence over gradient, layer color,
  and text paint while clipping, hit-testing, and source-over compositing remain
  bounded and consistent.
- Apply synthetic horizontal stretch only within the bitmap dimension budget.
  Malformed, unsupported, oversized, or over-stretched bitmap data falls back
  to the existing variable-outline, outline, or fontdue path.
- Preserve COLR precedence, shaping, spacing, font fallback, WOFF/WOFF2
  normalization, two-crate ownership, and explicit CDP backend behavior.

## Tradeoffs and remaining gates

The implementation covers the raster formats exposed by the current
`ttf-parser` bitmap-image API and reuses the existing bounded PNG decoder. It
does not add SVG-in-font decoding, palette-selection APIs, bitmap variation
axes, hinting, font-display timing, indexed-PNG support beyond the existing
image decoder, or complete FontFace/Web IDL parity. The optional system-font
witness is skipped when no supported color-bitmap font is installed; deterministic
format and compositor tests cover the bounded local contract.

## Verification

- `cargo fmt --all -- --check` and `git diff --check` passed.
- `RUST_MIN_STACK=8388608 cargo check -p glass-browser --lib --locked` passed.
- The focused bitmap regression passed 4/4.
- The native raster regression group passed 32/32.
- The serial native-engine unit group passed 448/448.
- The full locked `glass-browser` library regression passed 1,247 tests with
  1 ignored.
- Static review passed after selected TTC/OTC collection-index propagation was
  added to bitmap lookup.
- No remote CI, push, release, tag, registry publication, native/CDP parity
  certification, or issue closure is implied by this local checkpoint.

## Touched implementation

- `crates/glass-browser/src/browser/native_engine/font.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
