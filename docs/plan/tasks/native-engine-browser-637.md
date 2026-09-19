# Native engine browser slice 637: physical border color variables

Status: implementation, native-library, package, and documentation
verification complete locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now models bounded custom-property substitution for the
physical `border-color` shorthand. A standalone `var(--name)` retains one
side-indexed shorthand candidate per physical edge, and concrete one- to
four-color fallbacks expand through the existing top/right/bottom/left grammar.

Resolution preserves inherited aliases, invalid and cyclic custom-property
handling, concrete fallback parsing, and declaration-order precedence. The
single-side `border-*-color` grammar remains separate and continues to reject
multi-color raw custom-property values. Nested-variable and CSS-wide shorthand
fallbacks remain rejected. Full registered-property semantics, complete color
grammar, and complete CSS and Web IDL parity remain issue #40 gates.

## Focused coverage

- `border_color_shorthand_parser_accepts_custom_property_aliases_and_fallbacks`
  verifies side-indexed aliases, one- to four-color fallback expansion, and
  rejection of nested-variable or CSS-wide fallbacks.
- `border_color_shorthand_custom_properties_resolve_with_fallbacks` verifies
  direct shorthand values, inherited aliases, concrete fallbacks, invalid and
  cyclic custom properties, and declaration-order precedence.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked border_color -- --nocapture
```

Observed focused result: 12 passed, 0 failed, 1,389 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,400 passed, 0 failed, 1 ignored, 0 filtered out.

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

Observed documentation gate result: 1,287 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.
