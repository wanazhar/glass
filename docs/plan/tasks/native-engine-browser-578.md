# Native-engine browser slice 578: inherited optical sizing variables

## Scope

Issue #40 requires native CSS inheritance and bounded custom-property
substitution for the font controls used by native layout, script projection,
and text shaping. This slice extends that contract to inherited
`font-optical-sizing` without widening the native parser beyond its supported
`auto` and `none` values.

## Implementation

- `FontOpticalSizingDeclarationValue` keeps concrete values separate from
  bounded `var(--name)` and concrete fallback declarations.
- Computed style resolves inherited aliases, CSS-wide mapped declarations,
  invalid custom-property values, and cyclic custom-property values before
  applying the existing `font-optical-sizing` initial value (`auto`).
- Unsupported nested variable fallback grammar remains fail-closed and is
  recorded as an explicit issue #40 gate rather than silently approximated.

## Focused coverage

- `font_optical_sizing_parser_accepts_only_supported_keywords` validates the
  standalone variable form, concrete fallback form, and nested fallback
  rejection alongside the existing keyword grammar.
- `inherited_font_optical_sizing_custom_properties_resolve_with_fallbacks`
  validates inherited aliases, missing fallback, invalid-mapped, cyclic, and
  CSS-wide mapped values through computed native layout styles.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
  font_optical_sizing -- --nocapture`: 4 passed.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
  1306 passed, 1 ignored, 1305 filtered.
- Package gates passed:
  `scripts/check-rust-workspace.sh fast-check`, the locked `glass-browser`
  binary build, the locked `glass-dev` binary build, and locked Cargo metadata.
- Documentation and hygiene gates passed: 93 current guides routed/audited,
  19 substantive contracts, 1228 Markdown files, 346 full-product MCP tools
  (101 browser-only), 17 examples, 22 public modules, 83 current release
  documents, 63 previous-version hits, 1376 semantic audit hits, zero
  current-claim failures, `cargo fmt --all -- --check`, and `git diff --check`.
