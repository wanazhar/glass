# Native engine browser slice 616: logical border-width variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in the
logical `border-block-width`, `border-block-start-width`,
`border-block-end-width`, `border-inline-width`, `border-inline-start-width`,
and `border-inline-end-width` declarations. Standalone `var(--name)` and pixel
or CSS-wide fallbacks are retained through the local cascade; logical
one-to-two-value pair fallbacks preserve the existing start/end expansion.

Resolution preserves inherited custom-property aliases, CSS-wide reset and
inherit mappings, local cascade precedence, direction-aware projection,
`revert-layer` rollback, invalid-value fallback, and bounded cyclic-value
failure before border composition. Physical border-width declarations retain
their existing typed path and behavior. Unsupported nested variable grammar
remains fail-closed.

## Focused coverage

- `logical_border_width_parser_accepts_custom_property_aliases_and_fallbacks`
  verifies logical pair fallbacks, physical-side fallback parsing used by
  logical longhands, nested-variable rejection, CSS-wide mixed-value rejection,
  and `revert-layer` parsing.
- `logical_border_width_custom_properties_resolve_with_fallbacks` verifies
  inherited pair aliases, pair fallback expansion, invalid and cyclic custom
  property fallback, CSS-wide `initial` reset mapping, and direction-aware
  inline longhand projection through computed style.
- Existing logical border-width parser, important-priority, cascade,
  `revert-layer`, physical projection, border composition, and geometry tests
  remain green.

Focused command:

```text
cargo fmt --all && RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked logical_border_width -- --nocapture
```

Observed focused result: 4 passed, 0 failed, 1,355 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,359 passed, 0 failed, 1 ignored, 1,358 filtered
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

Observed documentation gate result: 1,266 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.

The remaining issue #40 gates stay explicit: nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity.
