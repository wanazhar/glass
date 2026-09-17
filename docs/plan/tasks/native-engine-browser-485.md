# Native engine browser-complete slice 485: real font metrics and rasterization

- Status: complete
- Scope: `native-engine` / explicit system-font text metrics and glyph paint
- Issue: #40
- Depends on: [native-engine-browser-484](native-engine-browser-484.md)

## Objective

Replace the native engine's fixed-cell text geometry and bitmap replay for
explicitly opted-in font styles with measured system-font metrics and real
glyph coverage, while preserving the established fallback contract for
existing pages and deterministic fixtures.

## Contract

- Inherited `font-family` accepts a bounded ordered list of up to four generic
  or named families; inherited `font-size` accepts positive integer pixel
  values from 1 through 256.
- An explicit supported family selects the best matching deterministic
  platform candidate, including normal/bold and normal/italic faces when
  available. Missing named faces and unavailable platform candidates fail
  closed to the native fixed-cell text owner.
- Selected faces contribute real per-character advances, pair kerning,
  horizontal line metrics, and rasterized coverage to wrapping, intrinsic
  width, line height, display-list paint, clipping, decorations, scrolling,
  hit geometry, and PNG replay.
- Real glyph data is carried as immutable bounded `GlyphRun` display-command
  state; the existing fixed 5x7 ASCII replay remains the output for elements
  whose computed family list is the compatibility fallback.
- Font loading is cached per process and uses only the checked-in
  platform-specific candidate paths. It does not scan arbitrary directories,
  execute font code, or create a second resource/network owner.

## Implementation

- Add the optional `fontdue` dependency behind the existing `native-engine`
  feature with default features disabled and SIMD/std enabled; update the
  lockfile to record the minimal parser/rasterizer dependency path.
- Add a bounded CSS family-list value with generic aliases, stable named-family
  hashing, CSS-wide inherited reset handling, and integer pixel-size parsing.
- Add a cached native font book with Linux, macOS, and Windows candidate tables;
  resolve face weight/style and calculate advances, kerning, ascent, and line
  height through fontdue.
- Thread selected metrics through DOM style lookup and inline layout, then
  emit glyph coverage in a new immutable display command while retaining the
  old `TextRun` path for fallback-compatible pages.
- Rasterize coverage with clipped source-over alpha and preserve the existing
  decoration, skip-ink, scrolling, opacity, and capture consumers.
- Add CSS parser, fallback-compatibility, real-font layout/display, and
  non-white raster coverage tests, including exhaustive display-command
  handling in existing regressions.

## Tradeoffs and remaining scope

This slice uses a small pure-Rust parser/rasterizer and an allowlisted font
table, so it is deterministic and avoids platform font-service or arbitrary
filesystem behavior. The dependency adds parser/rasterizer compile work and
glyph coverage memory, while keeping the dependency feature-gated so a
minimal non-native build does not include it. Fontdue supplies glyph metrics
and bitmaps but is not a shaping engine; placement is character-by-character
with pair kerning. The fallback path is deliberately retained to protect
existing fixed-cell fixtures during the transition.

`@font-face` and file/network font resources, full installed-font discovery,
font fallback across missing glyphs, relative/em/percentage/`calc()` sizes,
numeric weight interpolation, variable fonts, ligatures and GSUB/GPOS,
grapheme-safe wrapping, bidi/writing modes, language-specific line breaking,
font hinting/color management, and full browser text/Web IDL parity remain
issue #40 gates.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --lib --locked font_`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_real_font_metrics_feed_layout_and_glyph_paint_when_available`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_display_list_is_revisioned_deterministic_and_visibility_aware`
- documentation truth/depth/shortcut/coverage audits
- `git diff --check`
