# Native-engine browser slice 567: composite font-size variables

Status: implementation checkpoint; issue #40 remains open.

## Objective

Extend the bounded native inherited `font-size` custom-property path so
`var()` references and fallbacks can participate in a complete `calc()`
expression instead of only appearing as a standalone declaration.

## Scope

- Parse a bounded complete `calc()` expression containing custom-property
  references while preserving CSS custom-property name case.
- Expand mapped custom properties, nested references, missing references, and
  invalid mapped values with checked fallback selection.
- Resolve the expanded expression through the existing bounded fixed-point
  `font-size` calculation path, including parent-relative values and fallback
  fragments inside the larger expression.
- Bound expression bytes, substituted output bytes, and recursive variable
  expansion; reject malformed, non-ASCII, cyclic, and over-budget input
  fail-closed.
- Preserve direct custom-property behavior, cascade precedence, inherited maps,
  inline overrides, and the checked one-fallback form from slice 566.
- Keep complete CSS variable token grammar, registered custom properties,
  CSSOM mutation, and complete CSS variable/Web IDL parity as explicit issue #40
  gates.

## Verification

Focused checks passed:

- `font_size_parser_accepts_composite_custom_property_calculations`
- `font_size_variable_resolution_remains_bounded_and_fail_closed`
- `inherited_font_size_custom_properties_resolve_with_inline_override`

The full native library suite passed with
`RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
1295 tests passed across two suites, one ignored, and 1294 filtered.

The locked workspace fast check, `glass-browser` binary build, `glass-dev`
binary build, and locked metadata generation passed:

- `scripts/check-rust-workspace.sh fast-check`
- `cargo build -p glass-browser --bin glass-browser --locked`
- `cargo build -p glass-dev --bin glass --locked`
- `cargo metadata --no-deps --format-version 1 --locked`

Documentation depth, coverage, and release-truth checks passed with the exact
counts recorded below after this task document was added:

- depth: 93 current guides and 19 substantive contracts
- coverage: 1217 Markdown files, 346 full-product MCP tools, 101 browser-only
  tools, 17 examples, and 22 public modules
- release truth: 83 current documents, 63 previous-version hits, 1367
  semantic audit hits, and zero current-claim failures

`cargo fmt --all -- --check` and `git diff --check` passed.
