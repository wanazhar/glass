# Native engine browser slice 636: logical border color variables

Status: implementation, native-library, package, and documentation
verification complete locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now models bounded custom-property substitution for the
logical `border-block-color` and `border-inline-color` pairs. Standalone
`var(--name)` values retain the logical side index through alias chains, and
concrete two-color fallbacks resolve to the corresponding logical start/end
color before direction-aware projection to physical borders.

Resolution preserves inherited aliases, invalid and cyclic custom-property
handling, concrete fallback parsing, and declaration-order precedence. Nested
variable fallbacks and CSS-wide fallback tokens remain rejected; physical
single-side color substitution keeps its existing single-color grammar. Full
registered-property semantics, complete color grammar, and complete CSS and
Web IDL parity remain issue #40 gates.

## Focused coverage

- `logical_border_color_parser_accepts_custom_property_aliases_and_fallbacks`
  verifies standalone aliases, two-color concrete fallbacks, and rejection of
  nested-variable or CSS-wide fallbacks.
- `logical_border_color_custom_properties_resolve_with_fallbacks` verifies
  direction-aware block/inline projection, inherited aliases, concrete
  fallbacks, invalid and cyclic custom properties, and declaration-order
  precedence.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked logical_border_color -- --nocapture
```

Observed focused result: 4 passed, 0 failed, 1,395 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,398 passed, 0 failed, 1 ignored, 0 filtered out.

## Package and documentation gates

Package gate:

```text
scripts/check-rust-workspace.sh fast-check && cargo build -p glass-browser --bin glass-browser --locked && cargo build -p glass-dev --bin glass --locked && cargo metadata --no-deps --format-version 1 --locked >/dev/null
```

Documentation gate:

```text
python3 scripts/check-documentation-depth.py && python3 scripts/check-documentation-coverage.py && python3 scripts/check-release-documentation.py && cargo fmt --all -- --check && git diff --check
```

Observed package gate result: the fast workspace check, locked
`glass-browser` and `glass-dev` binary builds, and locked metadata validation
all passed without warnings.

Observed documentation gate result: 1,286 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.
