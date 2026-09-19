# Native-engine browser slice 573: inherited font weight variables

## Objective

Extend the bounded native CSS custom-property path to inherited
`font-weight` declarations while preserving relative-weight computation and
fail-closed handling for invalid or cyclic values.

## Scope

- Store standalone `var(--name)` and concrete `var(--name, weight)` values in
  the native font-weight declaration model.
- Resolve inherited, nested, missing-fallback, invalid, cyclic, and CSS-wide
  mapped values with the existing bounded custom-property depth.
- Preserve absolute numeric weights, `normal`, `bold`, `lighter`, `bolder`,
  `inherit`, `unset`, `initial`, `revert`, and `revert-layer` semantics.
- Keep complete shorthand substitution, non-concrete fallback grammar,
  registered properties, full CSS variable grammar, and complete CSS/Web IDL
  parity as issue #40 gates.

## Focused coverage

- `font_weight_property_parser_accepts_relative_keywords_only_at_property_level`
  validates standalone and concrete-fallback variable parsing and rejects
  nested fallback grammar.
- `inherited_font_weight_custom_properties_resolve_with_fallbacks` validates
  inherited, aliased, relative, missing-fallback, invalid-mapped, cyclic, and
  CSS-wide mapped values through computed native layout styles.

- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
  font_weight -- --nocapture`: 9 passed.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
  1301 passed, 1 ignored, 1300 filtered.
- Package gates passed:
  `scripts/check-rust-workspace.sh fast-check`, the locked `glass-browser`
  binary build, the locked `glass-dev` binary build, and locked Cargo metadata.
- Documentation and hygiene gates passed: 93 current guides routed/audited,
  19 substantive contracts, 1223 Markdown files, 346 full-product MCP tools
  (101 browser-only), 17 examples, 22 public modules, 83 current release
  documents, 63 previous-version hits, 1371 semantic audit hits, zero
  current-claim failures, `cargo fmt --all -- --check`, and `git diff --check`.
