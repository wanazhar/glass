# Native engine browser slice 611: box-sizing variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in
`box-sizing`. Standalone `var(--name)` and concrete
`var(--name, content-box|border-box)` fallbacks are retained through the local
cascade and resolved against inherited custom properties before computed-style
projection.

Resolution preserves the existing content-box fallback, explicit `inherit`,
CSS-wide reset mappings, local cascade precedence, `revert-layer` rollback,
invalid-value fallback, and bounded cyclic-value failure. Unsupported nested
variable grammar remains fail-closed. Declaration storage and resolution are
kept typed so ordinary keywords cannot be confused with unresolved custom
properties.

## Focused coverage

- `box_sizing_parser_accepts_custom_property_aliases_and_fallbacks` verifies
  bounded keywords, standalone custom-property references, concrete fallbacks,
  CSS-wide `initial`/`inherit` parsing, and nested-variable rejection.
- `box_sizing_custom_properties_resolve_with_fallbacks` verifies inherited
  aliases, concrete fallbacks, invalid and cyclic custom-property fallback,
  and CSS-wide `initial` reset mapping through computed-style projection.
- Existing box-model cascade, importance, reset, and `revert-layer` tests
  remain green.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked box_sizing -- --nocapture
```

Observed focused result: 2 passed, 0 failed, 1,347 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,349 passed, 0 failed, 1 ignored, 1,348 filtered out.

## Package and documentation gates

Package gate:

```text
scripts/check-rust-workspace.sh fast-check && cargo build -p glass-browser --bin glass-browser --locked && cargo build -p glass-dev --bin glass --locked && cargo metadata --no-deps --format-version 1 --locked >/dev/null
```

Observed package gate result: the fast workspace check, locked
`glass-browser` and `glass-dev` binary builds, and locked metadata validation
all passed.

Documentation gate:

```text
python3 scripts/check-documentation-depth.py && python3 scripts/check-documentation-coverage.py && python3 scripts/check-release-documentation.py && cargo fmt --all -- --check && git diff --check
```

Observed documentation gate result: 1,261 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.

The remaining issue #40 gates stay explicit: nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity.
