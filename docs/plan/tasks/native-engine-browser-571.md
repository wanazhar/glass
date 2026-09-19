# Native-engine browser slice 571: inherited border variables

## Objective

Extend the bounded native CSS custom-property path to border-color
longhands and logical color declarations without widening the accepted CSS
grammar or reintroducing a browser fallback.

## Scope

- Store standalone `var(--name)` and concrete `var(--name, color)` values in
the native border-color declaration model.
- Resolve inherited, nested, missing, invalid, and cyclic custom-property
values with the existing bounded resolution depth.
- Preserve `currentColor`, `inherit`, `unset`, `initial`, `revert`,
`revert-layer`, and the existing border cascade and paint semantics.
- Keep complete border shorthand substitution, composite color functions,
CSS-wide fallback values, registered properties, broader color properties,
full CSS variable grammar, and complete CSS/Web IDL parity as issue #40 gates.

## Focused coverage

- `border_color_parser_expands_physical_values_and_rejects_mixed_forms`
  validates standalone and concrete-fallback parser values for physical
  border-color declarations and rejects nested fallback grammar.
- `inherited_border_color_custom_properties_resolve_with_fallbacks`
  validates inherited, nested, missing-fallback, invalid-mapped, and cyclic
  values through computed native layout styles.

- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
  border_color -- --nocapture`: 8 passed.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
  1299 passed, 1 ignored, 1298 filtered.
- Package gates passed:
  `scripts/check-rust-workspace.sh fast-check`, the locked `glass-browser`
  binary build, the locked `glass-dev` binary build, and locked Cargo metadata.
- Documentation and hygiene gates passed: 93 current guides routed/audited,
  19 substantive contracts, 1221 Markdown files, 346 full-product MCP tools
  (101 browser-only), 17 examples, 22 public modules, 83 current release
  documents, 63 previous-version hits, 1369 semantic audit hits, zero
  current-claim failures, `cargo fmt --all -- --check`, and `git diff --check`.
