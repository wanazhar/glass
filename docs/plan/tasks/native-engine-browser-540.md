# Native-engine browser slice 540: palette color overrides

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Wire bounded `override-colors` descriptors from `@font-palette-values` into the
existing native COLR/CPAL solid and gradient paint path without changing font
matching, shaping, layout, or monochrome fallback behavior.

## Scope

- Parse bounded `override-colors: <palette-index> <color> ...` descriptors.
- Retain document-local overrides with the existing last-wins named palette
  rule and bounded `base-palette` selection.
- Apply exact CPAL-entry substitutions to admitted solid and gradient colors.
- Reject malformed, ambiguous, unsupported, or over-budget override mappings
  rather than silently painting the wrong color.
- Keep CSS keywords, custom-name CSSOM, unregistered-name fallback, and
  palette-count validation unchanged.

## Contract

- Override indices are non-negative bounded `u16` values; each rule admits at
  most 32 pairs and each document admits at most 16 palette-value rules.
- CSS colors reuse the existing bounded `NativeColor` grammar. The descriptor
  is invalid if any pair is malformed or duplicated.
- A named rule with unsupported overrides is not registered, so an unregistered
  or invalid selection fails closed to palette zero at paint time.
- The raster bridge substitutes only an exact, uniquely identifiable CPAL
  source color. A missing or ambiguous source mapping fails closed to the
  existing monochrome path; it never substitutes by nearest color or silently
  ignores the descriptor.
- Overrides affect COLR/CPAL paint colors only. Font matching, shaping, text
  metrics, transforms, clipping, compositing, display-list bounds, CSSOM, and
  raster budgets remain unchanged. No CDP fallback is introduced.

## Verification

- Focused `font_palette` filter: 7 passed.
- `colr_palette_overrides_change_solid_and_gradient_colors`: 1 passed.
- `named_font_palette_resolves_base_palette_for_text_metrics`: 1 passed.
- Serialized native library suite: 1263 passed, 1 ignored.
- Native integration `native_real_font_metrics_feed_layout_and_glyph_paint_when_available`:
  1 passed.
- `cargo check -p glass-browser --lib --locked` passed.
- `cargo check -p glass-dev --lib --bins --locked` passed.
- `cargo build -p glass-dev --bin glass --locked` passed.
- `cargo metadata --no-deps --format-version 1 --locked` passed.
- `cargo fmt --all -- --check` passed.
- Documentation depth, coverage, release-truth, and `git diff --check` gates
  passed: 1190 Markdown files, 83 current guides, 63 previous-version hits,
  1367 semantic-audit hits, 0 current-claim failures.
- No remote CI, push, release, or issue mutation was performed.
