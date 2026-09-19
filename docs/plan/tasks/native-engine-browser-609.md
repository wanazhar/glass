# Native engine browser slice 609: position offsets variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in the
position offset properties `top`, `right`, `bottom`, and `left`. Standalone
`var(--name)` and pixel or `auto` `var(--name, value)` fallbacks are retained
through the local cascade and resolved against inherited custom properties
before computed-style projection.

Resolution preserves pixel bounds, `auto` mapping, local cascade precedence,
CSS-wide reset mappings, `revert-layer` rollback, invalid-value fallback, and
bounded cyclic-value failure. Unsupported nested variable grammar remains
fail-closed.

## Focused coverage

- `position_offset_parser_accepts_custom_property_aliases_and_fallbacks`
  verifies bounded pixel values, standalone custom-property references,
  negative pixel fallbacks, CSS-wide `initial`/`inherit` parsing, and
  nested-variable rejection.
- `position_offsets_custom_properties_resolve_with_fallbacks` verifies
  inherited aliases across all four offset properties, negative and positive
  pixel fallbacks, invalid custom-property fallback, cyclic custom-property
  fallback, and CSS-wide `initial` reset mapping through computed-style
  projection.
- Existing position-offset bounds, cascade, reset, revert-layer, and
  declaration parsing tests remain green.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked position_offset -- --nocapture
```

Observed focused result: 2 passed, 0 failed, 1,343 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,345 passed, 0 failed, 1 ignored, 0 filtered out.

## Package and documentation gates

Package gate:

```text
scripts/check-rust-workspace.sh fast-check && cargo build -p glass-browser --bin glass-browser --locked && cargo build -p glass-dev --bin glass --locked && cargo metadata --no-deps --format-version 1 --locked >/dev/null
```

Observed package gate result: `scripts/check-rust-workspace.sh fast-check`,
the `glass-browser` and `glass-dev` binary builds, and locked metadata
validation all passed.

Documentation gate:

```text
python3 scripts/check-documentation-depth.py && python3 scripts/check-documentation-coverage.py && python3 scripts/check-release-documentation.py && cargo fmt --all -- --check && git diff --check
```

Observed documentation gate result: 1,259 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim
failures. Formatting and diff checks passed.

The remaining issue #40 gates stay explicit: nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity.
