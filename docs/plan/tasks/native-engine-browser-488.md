# Native engine browser-complete slice 488: ordered font fallback

- Status: complete
- Scope: `native-engine` / ordered CSS family fallback for missing glyphs
- Issue: #40
- Depends on: [native-engine-browser-487](native-engine-browser-487.md)

## Objective

Make an ordered `font-family` list functional for text whose first selected
face does not contain every source character, without turning font matching
into arbitrary filesystem discovery or changing the existing shaped-run fast
path.

## Contract

- The cached font book retains the best weight/style match for every declared
  generic or named family in source order. The family list remains bounded to
  the existing four entries.
- A face that has a real cmap glyph for every source character owns the whole
  run and continues through HarfRust shaping and indexed fontdue rasterization.
- If no one face covers a complete run, each source character selects the
  first declared face with a real cmap glyph. Kerning is applied only between
  adjacent characters selected from the same face, so cross-face joins do not
  borrow unrelated metrics.
- A character absent from every declared face uses the first admitted face's
  bounded replacement behavior; no new font directories, platform font
  services, network requests, or resource owners are introduced.
- Mixed-coverage runs retain the same `NativeFontRun`, display-list, clipping,
  decoration, scrolling, hit-geometry, and PNG replay consumers. Shaping is
  deliberately recovered per character for such a run, while a fully covered
  run keeps ligatures and positioned marks.

## Implementation

- Replace single-face matching with an ordered vector of best matching faces
  selected from the parsed family list.
- Use fontdue's cmap lookup to choose a face per source character for metric
  measurement and the existing character recovery rasterizer.
- Keep whole-run shaping tied to the first face that covers all characters and
  carry that face index through the shaped-run raster owner.
- Add focused coverage for ordered candidate retention and preserve the
  existing real-font, direction, and display-list regressions.

## Tradeoffs and remaining scope

The ordered face vector adds small per-metrics memory and lookup work, while
the common fully covered run remains a single-face HarfRust path. Mixed runs
lose cross-character shaping across a face boundary by design; this avoids
inventing ligatures from incompatible font tables and keeps the fallback
owner deterministic. The primary-face replacement for a universally missing
character is observable but bounded and does not claim a universal glyph
fallback service.

`@font-face` file/network resources, arbitrary installed-font discovery,
font-display/loading events, variable-font instances, color fonts, CSS
font-feature settings, mixed bidi and writing modes, grapheme-safe line
breaking, and complete browser text/Web IDL parity remain issue #40 gates.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --lib --locked font_`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_css_direction_reaches_real_font_raster_coordinates`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_real_font_metrics_feed_layout_and_glyph_paint_when_available`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_display_list_is_revisioned_deterministic_and_visibility_aware`
- documentation truth/depth/shortcut/coverage audits
- `git diff --check`
