# Native-engine browser slice 576: inherited font family variables

## Objective

Extend the bounded native CSS custom-property path to inherited
`font-family` declarations while preserving ordered family fallback lists and
fail-closed handling for invalid or cyclic values.

## Scope

- Store standalone `var(--name)` and bounded comma-list
  `var(--name, family, fallback)` values in the native font-family declaration
  model.
- Resolve inherited, aliased, missing-fallback, invalid, cyclic, and CSS-wide
  mapped values with the existing bounded custom-property depth.
- Preserve quoted names, generic families, ordered fallback lists, `inherit`,
  `unset`, `initial`, `revert`, and `revert-layer` semantics and computed-style
  output.
- Keep complete shorthand substitution, nested variable grammar, registered
  properties, installed-font discovery, full CSS variable grammar, and complete
  CSS/Web IDL parity as issue #40 gates.

## Focused coverage

- `font_family_parser_keeps_ordered_bounded_fallbacks` validates the bounded
  family-list grammar, standalone and comma-list fallback variable parsing, and
  rejects nested fallback variables.
- `inherited_font_family_custom_properties_resolve_with_fallbacks` validates
  inherited aliases, missing fallback, invalid-mapped, cyclic, and CSS-wide
  mapped values through computed native layout styles.

- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
  font_family -- --nocapture`: 3 passed.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
  1304 passed, 1 ignored, 1303 filtered.
- Package gates passed:
  `scripts/check-rust-workspace.sh fast-check`, the locked `glass-browser`
  binary build, the locked `glass-dev` binary build, and locked Cargo metadata.
- Documentation and hygiene gates passed: 93 current guides routed/audited,
  19 substantive contracts, 1226 Markdown files, 346 full-product MCP tools
  (101 browser-only), 17 examples, 22 public modules, 83 current release
  documents, 63 previous-version hits, 1374 semantic audit hits, zero
  current-claim failures, `cargo fmt --all -- --check`, and `git diff --check`.
