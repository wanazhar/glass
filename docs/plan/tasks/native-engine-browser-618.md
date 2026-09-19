# Native engine browser slice 618: border-radius variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in
`border-radius` and the four physical corner longhands:
`border-top-left-radius`, `border-top-right-radius`,
`border-bottom-right-radius`, and `border-bottom-left-radius`. Standalone
`var(--name)` and pixel or bounded shorthand fallbacks are retained through
the local cascade.

Resolution preserves inherited custom-property aliases, CSS-wide reset and
inherit mappings, local cascade precedence, corner-specific fallback
resolution, `revert-layer` rollback, invalid-value fallback, bounded cyclic
value failure, and logical-corner projection before computed border geometry.
The existing bounded pixel-only radius grammar remains unchanged; percentage
and slash syntax remain fail-closed in this slice. Unsupported registered
properties and full variable grammar remain explicit issue #40 gates.

## Focused coverage

- `border_radius_parser_accepts_custom_property_aliases_and_fallbacks`
  verifies shorthand aliases, bounded shorthand fallbacks, corner-specific
  fallbacks, nested-variable rejection, and `revert-layer` parsing.
- `border_radius_custom_properties_resolve_with_fallbacks` verifies inherited
  shorthand aliases, shorthand fallback expansion, invalid and cyclic custom
  property fallback, CSS-wide `initial` reset mapping, and corner longhand
  projection through computed style.
- Existing radius shorthand, corner precedence, important-priority,
  `revert-layer`, CSS-wide, logical-corner projection, composition, and
  geometry tests remain green.

Focused command:

```text
cargo fmt --all && RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked border_radius -- --nocapture
```

Observed focused result: 13 passed, 0 failed, 1,350 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,363 passed, 0 failed, 1 ignored, 1,362 filtered
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

Observed documentation gate result: 1,268 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.

The remaining issue #40 gates stay explicit: percentage and slash radius
syntax, registered properties, full CSS variable grammar, and complete
CSS/Web IDL parity.
