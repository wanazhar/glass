# Native-engine browser slice 539: custom palette base selection

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Wire named CSS palettes through `font-palette` and
`@font-palette-values` for bounded `base-palette` selection.

## Scope

- Accept bounded custom dashed-ident names in `font-palette`.
- Parse bounded `@font-palette-values --name { base-palette: N; }` rules and
  resolve the last rule for a name.
- Carry a resolved base palette into the existing COLR/CPAL rasterizer.
- Keep `normal`, `light`, and `dark` semantics unchanged.
- Reject unsupported `override-colors` declarations rather than silently
  pretending to render them; color overrides remain a later issue #40 gate.

## Contract

- Named palettes are inherited and remain visible through computed-style/CSSOM;
  an unregistered name fails closed to palette zero at paint time.
- `base-palette` is a bounded non-negative `u16`; face palette-count validation
  remains in the rasterizer and out-of-range values fail closed.
- Named base selection changes only COLR/CPAL palette colors. Shaping, metrics,
  font matching, variations, monochrome fallback, and raster budgets remain
  unchanged.
- Palette-rule storage is document-local and bounded; no CDP fallback or
  compatibility shim is introduced.

## Verification

- CSS custom-name parser, palette-rule storage/resolution, and inherited
  computed-style coverage: 6 passed with the `font_palette` filter.
- Direct CPAL base-index validation and COLR raster color selection: 11 passed
  with the `palette` filter.
- Serialized locked native library suite: 1261 passed, 1 ignored, using
  `RUST_MIN_STACK=8388608` and one test thread.
- Native integration gate
  `native_real_font_metrics_feed_layout_and_glyph_paint_when_available`: 1
  passed.
- `cargo check -p glass-browser --lib --locked`, `cargo check -p glass-dev
  --lib --bins --locked`, `cargo build -p glass-dev --bin glass --locked`, and
  locked metadata validation passed.
- Formatting, documentation-depth, documentation-coverage,
  release-truth, and diff checks passed: 93 current guides / 19 substantive
  contracts; 1189 Markdown files / 346 full-product MCP tools / 101
  browser-only tools / 17 examples / 22 public modules; current-release
  claim failures 0; semantic audit hits 1367; previous-version hits 63.
