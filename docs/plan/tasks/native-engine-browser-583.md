# Native-engine browser slice 583: inherited ligature variables

## Scope

Issue #40 requires native CSS inheritance and bounded custom-property
substitution for the font controls used by native layout, script projection,
and text shaping. This slice extends that contract to inherited
`font-variant-ligatures` without widening the native parser beyond its four
feature-group controls.

## Implementation

- `FontVariantLigaturesDeclarationValue` keeps concrete group values separate
  from bounded `var(--name)` and keyword-list fallback declarations.
- The existing `font-variant` shorthand maps its concrete ligature component
  into the new declaration wrapper without changing the other shorthand
  components.
- Computed style resolves inherited aliases, CSS-wide mapped declarations,
  invalid custom-property values, and cyclic custom-property values before
  applying the existing default feature-group values.
- Unsupported nested variable fallback grammar remains fail-closed and is
  recorded as an explicit issue #40 gate rather than silently approximated.

## Focused coverage

- `font_variant_ligatures_parser_keeps_one_value_per_feature_group` validates
  the standalone variable form, keyword fallback, and nested fallback
  rejection alongside the existing exclusivity grammar.
- `inherited_font_variant_ligatures_custom_properties_resolve_with_fallbacks`
  validates inherited aliases, missing fallback, invalid-mapped, cyclic, and
  CSS-wide mapped values through computed native layout styles.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
  font_variant_ligatures -- --nocapture`: 5 passed.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
  1311 passed, 1 ignored, 1310 filtered.
- Package gates passed:
  `scripts/check-rust-workspace.sh fast-check`, the locked `glass-browser`
  binary build, the locked `glass-dev` binary build, and locked Cargo metadata.
- Documentation and hygiene gates passed: 93 current guides routed/audited,
  19 substantive contracts, 1233 Markdown files, 346 full-product MCP tools
  (101 browser-only), 17 examples, 22 public modules, 83 current release
  documents, 63 previous-version hits, 1381 semantic audit hits, zero
  current-claim failures, `cargo fmt --all -- --check`, and `git diff --check`.
