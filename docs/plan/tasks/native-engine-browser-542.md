# Native-engine browser slice 542: CSS font-display descriptor projection

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Carry the bounded CSS `font-display` descriptor from native `@font-face`
parsing into the page-realm `FontFace` projection without changing the existing
font admission or loading-status contract.

## Scope

- Accept the five CSS `font-display` keywords: `auto`, `block`, `swap`,
  `fallback`, and `optional`, case-insensitively and as one token.
- Store the normalized descriptor on each native `@font-face` rule.
- Serialize the descriptor as `display` on static `document.fonts` faces and
  include it in the stable static-face identity key.
- Preserve the existing status, admission, error, event, and dynamic
  `FontFace` source behavior.

## Contract

- Missing or unrecognized `font-display` values use the existing CSS diagnostic
  path; missing descriptors retain the CSS default `auto`.
- The page realm observes the normalized lowercase keyword through
  `FontFace.display` for static CSS faces. A descriptor change creates a new
  static-face identity rather than mutating an unrelated face.
- This slice carries descriptor data only. Native font loading remains
  admission-driven; block/swap/fallback/optional render timing, installed-font
  discovery, and complete FontFace/Web IDL parity remain issue #40 gates.
- No CDP fallback, network behavior, font bytes, or resource status semantics
  change.

## Verification

- Focused `font_display_parser` filter: 1 passed.
- Focused `css_font_face_refresh_dispatches_loading_error_for_new_rules`
  filter: 1 passed.
- `cargo check -p glass-browser --lib --locked` passed.
- Serialized native library suite: 1267 passed, 1 ignored.
- Native integration
  `native_real_font_metrics_feed_layout_and_glyph_paint_when_available`:
  1 passed.
- `cargo check -p glass-dev --lib --bins --locked` passed.
- `cargo build -p glass-dev --bin glass --locked` passed.
- `cargo metadata --no-deps --format-version 1 --locked` passed.
- `cargo fmt --all -- --check`, documentation depth, coverage,
  release-truth, and `git diff --check` gates passed: 1192 Markdown files,
  93 current guides, 19 substantive contracts, 63 previous-version hits,
  1367 semantic-audit hits, 0 current-claim failures.
- No remote CI, push, release, or issue mutation was performed.
