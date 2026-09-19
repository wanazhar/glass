# Native engine browser slice 613: margin variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in the
physical `margin` shorthand and physical `margin-top`, `margin-right`,
`margin-bottom`, and `margin-left` longhands. Standalone `var(--name)` and
concrete pixel or `auto` fallbacks are retained through the local cascade;
shorthand fallbacks expand through the existing one-to-four-value physical edge
rules.

Resolution preserves inherited custom-property aliases, explicit `inherit`,
CSS-wide reset mappings, local cascade precedence, auto-edge provenance,
`revert-layer` rollback, invalid-value fallback, and bounded cyclic-value
failure before computed-style projection. Logical margin declarations retain
their existing typed path and physical projection behavior is unchanged.
Unsupported nested variable grammar remains fail-closed.

## Focused coverage

- `margin_parser_accepts_custom_property_aliases_and_fallbacks` verifies
  bounded pixel and `auto` values, longhand aliases and fallbacks, shorthand
  aliases, one-to-four-value shorthand fallbacks, CSS-wide `initial`/`inherit`,
  and nested-variable rejection.
- `margin_custom_properties_resolve_with_fallbacks` verifies inherited
  shorthand aliases, auto-edge provenance, four-edge fallback expansion,
  longhand aliases, invalid and cyclic custom-property fallback, and CSS-wide
  `initial` reset mapping through computed-style projection.
- Existing box-model cascade, importance, reset, logical-margin, auto-margin,
  and `revert-layer` tests remain green.

Focused command:

```text
cargo fmt --all && RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked margin_ -- --nocapture
```

Observed focused result: 3 passed, 0 failed, 1,350 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,353 passed, 0 failed, 1 ignored, 1,352 filtered out.

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

Observed documentation gate result: 1,263 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.

The remaining issue #40 gates stay explicit: nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity.
