# Native-engine browser slice 541: SVG-in-font glyphs

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Admit bounded OpenType `SVG ` glyph documents into the native font raster path
using the existing SVG DOM and surface rasterizer, without allowing external
resources or changing outline, bitmap, COLR, or variable-font fallback.

## Scope

- Read `ttf-parser` SVG glyph documents for the selected glyph.
- Accept bounded UTF-8 SVG and bounded gzip-compressed SVGZ payloads.
- Map a valid font viewBox to a font-size pixel viewport and retain RGBA pixels
  as a bitmap glyph with bounded coverage.
- Reject malformed viewBoxes, unsafe external-resource constructs, oversized
  sources, decompression output, dimensions, or surfaces and fall through to
  existing color/bitmap/outline behavior.
- Keep shaping, metrics, palette overrides, variation settings, stretch,
  clipping, compositing, and raster budgets unchanged.

## Contract

- One SVG document is limited to 512 KiB compressed or plain source and its
  decompressed form; output dimensions are derived from bounded viewBox units
  and the selected face's units-per-em, capped at 1024 pixels per axis and the
  existing bitmap byte budget.
- SVGZ uses bounded gzip reads; decompression never grows past the source cap.
- Only static local SVG content is admitted. Scripts, external images/objects,
  `<use>`, `href`, `url()`, and imports fail closed before DOM rasterization.
- A valid SVG glyph wins before COLR/CPAL and bitmap strikes; failed SVG
  admission falls through to existing COLR, bitmap, variable-outline, or
  fontdue-outline paths. SVG colors are not CPAL palette substitutions.
- No CDP fallback or network access is introduced.

## Verification

- Focused `svg_font` filter: 3 passed.
- Serialized native library suite: 1266 passed, 1 ignored.
- Native integration `native_real_font_metrics_feed_layout_and_glyph_paint_when_available`:
  1 passed.
- `cargo check -p glass-browser --lib --locked` passed.
- `cargo check -p glass-dev --lib --bins --locked` passed.
- `cargo build -p glass-dev --bin glass --locked` passed.
- `cargo metadata --no-deps --format-version 1 --locked` passed.
- `cargo fmt --all -- --check` passed.
- Documentation depth, coverage, release-truth, and `git diff --check` gates
  passed: 1191 Markdown files, 83 current guides, 63 previous-version hits,
  1367 semantic-audit hits, 0 current-claim failures.
- No remote CI, push, release, or issue mutation was performed.
