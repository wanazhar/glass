# Native-engine browser slice 575: inherited font stretch variables

## Objective

Extend the bounded native CSS custom-property path to inherited
`font-stretch` declarations while preserving the existing named and percentage
single-value grammar and fail-closed handling for invalid or cyclic values.

## Scope

- Store standalone `var(--name)` and concrete `var(--name, stretch)` values in
  the native font-stretch declaration model.
- Resolve inherited, aliased, missing-fallback, invalid, cyclic, and CSS-wide
  mapped values with the existing bounded custom-property depth.
- Preserve named stretch values, bounded percentages, `inherit`, `unset`,
  `initial`, `revert`, and `revert-layer` semantics and computed-style output.
- Keep two-value descriptor ranges, complete shorthand substitution,
  non-concrete fallback grammar, registered properties, full CSS variable
  grammar, and complete CSS/Web IDL parity as issue #40 gates.

## Focused coverage

- `font_stretch_parser_normalizes_keywords_percentages_and_ranges` validates
  the base grammar, standalone and concrete-fallback variable parsing, and
  rejects nested fallback grammar.
- `inherited_font_stretch_custom_properties_resolve_with_fallbacks` validates
  inherited aliases, missing fallback, invalid-mapped, cyclic, and CSS-wide
  mapped values through computed native layout styles.

- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
  font_stretch -- --nocapture`: 5 passed.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
  1303 passed, 1 ignored, 1302 filtered.
- Package gates passed:
  `scripts/check-rust-workspace.sh fast-check`, the locked `glass-browser`
  binary build, the locked `glass-dev` binary build, and locked Cargo metadata.
- Documentation and hygiene gates passed: 93 current guides routed/audited,
  19 substantive contracts, 1225 Markdown files, 346 full-product MCP tools
  (101 browser-only), 17 examples, 22 public modules, 83 current release
  documents, 63 previous-version hits, 1373 semantic audit hits, zero
  current-claim failures, `cargo fmt --all -- --check`, and `git diff --check`.
