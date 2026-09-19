# Native engine browser slice 620: gap variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in
`gap`, `row-gap`, and `column-gap`. Standalone `var(--name)` values, one- or
two-value shorthand fallbacks, and single-component fallbacks are retained
through the fixed row/column cascade candidates.

Resolution projects shorthand aliases independently into row and column
components. Inherited custom-property aliases, invalid and cyclic fallback
handling, CSS-wide reset mappings, longhand precedence, and existing
`revert-layer` rollback stay within the current cascade owners. The existing
bounded non-negative integer-pixel grammar remains unchanged. Percentage,
relative-unit, calculation, nested-variable, registered-property, full CSS
variable grammar, and complete CSS/Web IDL parity remain explicit issue #40
gates.

## Focused coverage

- `gap_parser_accepts_custom_property_aliases_and_fallbacks` verifies
  shorthand aliases, two-value shorthand fallback, single-component fallback,
  row/column storage, and nested-variable rejection.
- `gap_custom_properties_resolve_with_fallbacks` verifies inherited aliases,
  shorthand fallback, invalid and cyclic fallback, CSS-wide reset, and
  independent row/column longhand projection through computed style.
- Existing gap parser, cascade, CSS-wide, `revert-layer`, precedence, and
  flex/grid consumer tests remain green.

Focused commands:

```text
cargo fmt --all && RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked gap_parser_accepts_custom_property_aliases_and_fallbacks -- --nocapture
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked gap_custom_properties_resolve_with_fallbacks -- --nocapture
```

Observed focused results: parser 1 passed, 0 failed, 1,366 filtered out;
resolution 1 passed, 0 failed, 1,366 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,367 passed, 0 failed, 1 ignored, 1,366 filtered
out.

## Package and documentation gates

Package gate:

```text
scripts/check-rust-workspace.sh fast-check && cargo build -p glass-browser --bin glass-browser --locked && cargo build -p glass-dev --bin glass --locked && cargo metadata --no-deps --format-version 1 --locked >/dev/null
```

Observed package gate result: the fast workspace check, locked
`glass-browser` and `glass-dev` binary builds, and locked metadata validation
all passed without warnings.

Documentation gate:

```text
python3 scripts/check-documentation-depth.py && python3 scripts/check-documentation-coverage.py && python3 scripts/check-release-documentation.py && cargo fmt --all -- --check && git diff --check
```

Observed documentation gate result: 1,270 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.

The remaining issue #40 gates stay explicit: percentage, relative-unit, and
calculation grammar; nested variable grammar; registered properties; full CSS
variable grammar; and complete CSS/Web IDL parity.
