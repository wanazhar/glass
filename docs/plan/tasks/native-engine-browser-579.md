# Native-engine browser slice 579: inherited font palette variables

## Scope

Issue #40 requires native CSS inheritance and bounded custom-property
substitution for the font controls used by native layout, script projection,
and text shaping. This slice extends that contract to inherited `font-palette`
without widening the native parser beyond its supported normal, light, dark,
and named palette values.

## Implementation

- `FontPaletteDeclarationValue` keeps concrete values separate from bounded
  `var(--name)` and concrete fallback declarations.
- Computed style resolves inherited aliases, CSS-wide mapped declarations,
  invalid custom-property values, and cyclic custom-property values before
  applying the existing `font-palette` initial value (`normal`).
- Unsupported nested variable fallback grammar remains fail-closed and is
  recorded as an explicit issue #40 gate rather than silently approximated.

## Focused coverage

- `font_palette_parser_accepts_keywords_and_custom_names` validates the
  standalone variable form, named fallback form, and nested fallback
  rejection alongside the existing palette grammar.
- `inherited_font_palette_custom_properties_resolve_with_fallbacks` validates
  inherited aliases, missing fallback, invalid-mapped, cyclic, and CSS-wide
  mapped values through computed native layout styles.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
  font_palette -- --nocapture`: 8 passed.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
  1307 passed, 1 ignored, 1306 filtered.
- Package gates passed:
  `scripts/check-rust-workspace.sh fast-check`, the locked `glass-browser`
  binary build, the locked `glass-dev` binary build, and locked Cargo metadata.
- Documentation and hygiene gates passed: 93 current guides routed/audited,
  19 substantive contracts, 1229 Markdown files, 346 full-product MCP tools
  (101 browser-only), 17 examples, 22 public modules, 83 current release
  documents, 63 previous-version hits, 1377 semantic audit hits, zero
  current-claim failures, `cargo fmt --all -- --check`, and `git diff --check`.
