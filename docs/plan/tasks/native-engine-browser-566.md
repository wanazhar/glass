# Native-engine browser slice 566: font-size variable fallbacks

Status: implementation checkpoint; issue #40 remains open.

## Objective

Extend the bounded native inherited `font-size` custom-property path so
`var(--name, fallback)` remains useful when the referenced property is missing
or does not resolve to a valid native font-size value.

## Scope

- Parse a bounded top-level `var()` argument list with one custom-property name
  and one fallback value.
- Accept fallback absolute, parent-relative, root-relative, viewport-relative,
  and existing bounded `calc()` values.
- Accept CSS-wide fallback keywords with the native inherited/initial semantics.
- Resolve the referenced custom property first, then use the fallback when the
  property is missing, malformed, invalid, or exceeds the checked resolution
  depth.
- Preserve direct `var(--name)` behavior, cascade precedence, inherited maps,
  and fail-closed handling for unsupported nested fallback expressions.
- Keep composite `var()` substitution inside larger `calc()` expressions,
  registered custom properties, CSSOM mutation, and complete CSS variable/Web
  IDL parity as explicit issue #40 gates.

## Verification

Focused checks passed:

- `font_size_parser_accepts_custom_property_fallbacks`
- `inherited_font_size_custom_properties_resolve_with_inline_override`

The full native library suite passed with
`RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`:
1293 tests passed across two suites, one ignored, and 1292 filtered.

The locked workspace fast check, `glass-browser` binary build, `glass-dev`
binary build, and locked metadata generation passed:

- `scripts/check-rust-workspace.sh fast-check`
- `cargo build -p glass-browser --bin glass-browser --locked`
- `cargo build -p glass-dev --bin glass --locked`
- `cargo metadata --no-deps --format-version 1 --locked`

Documentation depth passed with 93 current guides and 19 substantive
contracts; coverage passed with 1216 Markdown files, 346 full-product MCP
tools, 101 browser-only tools, 17 examples, and 22 public modules; release
truth passed with 83 current documents, 63 previous-version hits, 1367
semantic audit hits, and zero current-claim failures.

`cargo fmt --all -- --check` and `git diff --check` passed.
