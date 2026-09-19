# Native-engine browser slice 574: inherited font style variables

## Objective

Extend the bounded native CSS custom-property path to inherited
`font-style` declarations while preserving the existing normal/italic grammar
and fail-closed handling for invalid or cyclic values.

## Scope

- Store standalone `var(--name)` and concrete `var(--name, style)` values in
  the native font-style declaration model.
- Resolve inherited, aliased, missing-fallback, invalid, cyclic, and CSS-wide
  mapped values with the existing bounded custom-property depth.
- Preserve `normal`, `italic`, `inherit`, `unset`, `initial`, `revert`, and
  `revert-layer` semantics and the existing computed-style projection.
- Keep `oblique` angles, complete shorthand substitution, non-concrete
  fallback grammar, registered properties, full CSS variable grammar, and
  complete CSS/Web IDL parity as issue #40 gates.

## Focused coverage

- `font_style_parser_accepts_only_normal_and_italic` validates the bounded
  base grammar, standalone and concrete-fallback variable parsing, and rejects
  nested fallback grammar.
- `inherited_font_style_custom_properties_resolve_with_fallbacks` validates
  inherited aliases, missing fallback, invalid-mapped, cyclic, and CSS-wide
  mapped values through computed native layout styles.

- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
  font_style -- --nocapture`: 3 passed.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
  1302 passed, 1 ignored, 1301 filtered.
- Package gates passed:
  `scripts/check-rust-workspace.sh fast-check`, the locked `glass-browser`
  binary build, the locked `glass-dev` binary build, and locked Cargo metadata.
- Documentation and hygiene gates passed: 93 current guides routed/audited,
  19 substantive contracts, 1224 Markdown files, 346 full-product MCP tools
  (101 browser-only), 17 examples, 22 public modules, 83 current release
  documents, 63 previous-version hits, 1372 semantic audit hits, zero
  current-claim failures, `cargo fmt --all -- --check`, and `git diff --check`.
