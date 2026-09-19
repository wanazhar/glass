# Native-engine browser slice 569: inherited color variables

Status: implementation checkpoint; issue #40 remains open.

## Objective

Extend the bounded native custom-property path beyond `font-size` so an
inherited `color` declaration can consume a custom property and a concrete
`var()` fallback while preserving the existing cascade and fail-closed rules.

## Scope

- Parse standalone `color: var(--name)` and `color: var(--name, color)`
  declarations with bounded custom-property names and concrete color fallbacks.
- Resolve inherited custom properties, nested custom-property references,
  missing references, malformed mapped values, cyclic references, and bounded
  fallback selection before normal native color inheritance.
- Preserve CSS-wide color keywords, currentColor behavior, cascade precedence,
  and the existing paint/display-list color model.
- Keep composite color functions, CSS-wide fallback values inside `var()`,
  registered custom properties, broader color properties, full CSS variable
  grammar, and complete CSS/Web IDL parity as explicit issue #40 gates.

## Verification

Focused checks passed:

- `local_paint_color_parser_accepts_css_wide_keywords_and_preserves_valid_values`
- `inherited_color_custom_properties_resolve_with_fallbacks`

The full native library suite passed with
`RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
1297 tests passed across two suites, one ignored, and 1296 filtered.

The locked workspace fast check, `glass-browser` binary build, `glass-dev`
binary build, and locked metadata generation passed:

- `scripts/check-rust-workspace.sh fast-check`
- `cargo build -p glass-browser --bin glass-browser --locked`
- `cargo build -p glass-dev --bin glass --locked`
- `cargo metadata --no-deps --format-version 1 --locked`

Documentation depth, coverage, and release-truth checks passed with the exact
counts recorded below after this task document was added:

- depth: 93 current guides and 19 substantive contracts
- coverage: 1219 Markdown files, 346 full-product MCP tools, 101 browser-only
  tools, 17 examples, and 22 public modules
- release truth: 83 current documents, 63 previous-version hits, 1367
  semantic audit hits, and zero current-claim failures

`cargo fmt --all -- --check` and `git diff --check` passed.
