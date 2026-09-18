# Native-engine browser slice 526: COLR/CPAL color glyph layers

Status: complete on the current source line.

## Objective

Make bounded palette-based color glyphs visible in the native software text
renderer. Preserve the existing HarfRust shaping and coverage-raster contract,
but carry a per-layer color from a font's COLR/CPAL paint data through native
glyphs into the final surface.

## Contract

- Recognize color glyphs from the existing `ttf-parser` face owner and paint
  palette 0 with bounded solid COLR layers.
- Reuse the existing outline supersampling, glyph bounds, clipping, hit-test,
  display-list, and alpha-blending paths; color layers must not change glyph
  advance, shaping, spacing, or fallback selection.
- Bound one color glyph to 32 layers and retain the existing outline point,
  raster dimension, font-byte, and output budgets.
- If a color paint requires gradients, transforms, clips, composite layers, or
  malformed data outside this slice, fail the color path closed and retain the
  existing monochrome outline fallback rather than partially painting it.
- Keep WOFF/WOFF2 normalization, variable coordinates, the content-process
  wire shape, the two-crate boundary, and the explicit CDP backend unchanged.

## Tradeoffs and boundary

This slice makes common solid-palette COLR glyphs usable without adding a new
font or graphics dependency. It deliberately does not claim COLR gradient or
transform parity, CBDT/CBLC or SVG-in-font decoding, palette selection APIs,
color variation-axis support, hinting, `font-display` timing, or complete
FontFace/Web IDL parity. Unsupported color paint remains recoverable through
the existing monochrome path.

## Verification

- `cargo fmt --all` and `git diff --check` passed.
- Locked scoped `glass-browser` library check passed after the ownership fix.
- The scoped native-engine unit batch passed `437/437` with one test thread in
  40.09 seconds under `RUST_MIN_STACK=8388608`; this includes the two
  color-specific tests. The separate color-only batch passed `2/2` (solid
  layer rendering plus unsupported-paint fallback) in 32.61 seconds including
  the locked build/check overhead.
- The positive fixture is the compact 4,028-byte
  `crates/glass-browser/tests/fixtures/colr-v0.ttf`, with its DejaVu-derived
  license file. `colr-1.ttf` is retained as a 21,568-byte upstream COLRv1
  negative fixture, with its upstream license file, to prove unsupported
  gradients/clips fail closed.
- An initial serial run without the repository's required stack-size setting
  reached 435/437 and then overflowed in the existing
  `cli::args::tests::native_engine_is_the_default_browser_runtime` harness
  test. The identical batch passed with `RUST_MIN_STACK=8388608`; the final
  evidence for this slice is that deterministic result above.
- No remote CI, push, release, tag, registry publication, or native/CDP parity
  certification is implied by this local slice.

## Touched implementation

- `crates/glass-browser/src/browser/native_engine/font.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/fixtures/colr-v0.ttf`
- `crates/glass-browser/tests/fixtures/colr-v0_LICENSE`
- `crates/glass-browser/tests/fixtures/colr-1.ttf`
- `crates/glass-browser/tests/fixtures/colr-1_LICENSE`
