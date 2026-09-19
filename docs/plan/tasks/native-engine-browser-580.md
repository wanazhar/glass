# Native-engine browser slice 580: inherited feature settings variables

## Scope

Issue #40 requires native CSS inheritance and bounded custom-property
substitution for the font controls used by native layout, script projection,
and text shaping. This slice extends that contract to inherited
`font-feature-settings` without widening the native parser beyond its bounded
OpenType tag/value list.

## Implementation

- `FontFeatureSettingsDeclarationValue` keeps concrete feature lists separate
  from bounded `var(--name)` and comma-list fallback declarations.
- Computed style resolves inherited aliases, CSS-wide mapped declarations,
  invalid custom-property values, and cyclic custom-property values before
  applying the existing empty-list initial value.
- Unsupported nested variable fallback grammar remains fail-closed and is
  recorded as an explicit issue #40 gate rather than silently approximated.

## Focused coverage

- `font_feature_settings_parser_keeps_bounded_last_values` validates the
  standalone variable form, comma-list concrete fallback, and nested fallback
  rejection alongside the existing ordered feature-list grammar.
- `inherited_font_feature_settings_custom_properties_resolve_with_fallbacks`
  validates inherited aliases, missing fallback, invalid-mapped, cyclic, and
  CSS-wide mapped values through computed native layout styles.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
  font_feature_settings -- --nocapture`: 4 passed.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
  1308 passed, 1 ignored, 1307 filtered.
- Package gates passed:
  `scripts/check-rust-workspace.sh fast-check`, the locked `glass-browser`
  binary build, the locked `glass-dev` binary build, and locked Cargo metadata.
- Documentation and hygiene gates passed: 93 current guides routed/audited,
  19 substantive contracts, 1230 Markdown files, 346 full-product MCP tools
  (101 browser-only), 17 examples, 22 public modules, 83 current release
  documents, 63 previous-version hits, 1378 semantic audit hits, zero
  current-claim failures, `cargo fmt --all -- --check`, and `git diff --check`.
