# Native engine browser slice 619: complete-border variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in the
complete physical `border` shorthand and the four physical side-border
shorthands: `border-top`, `border-right`, `border-bottom`, and `border-left`.
Standalone `var(--name)` values and concrete-color complete-border fallbacks
are retained through the existing local cascade and component projections.

Resolution projects a complete border alias through the width, style, and
color streams. Inherited aliases, invalid and cyclic values, CSS-wide reset
mappings, physical side precedence, and existing `revert-layer` behavior stay
fail-closed or resolve through the existing component owners. The existing
complete-border grammar remains unchanged. This bounded slice intentionally
rejects current-color and non-complete fallback forms; nested variable
grammar, registered properties, full CSS variable grammar, and complete
CSS/Web IDL parity remain explicit issue #40 gates.

## Focused coverage

- `border_parser_accepts_custom_property_aliases_and_fallbacks` verifies
  standalone aliases, concrete-color complete-border fallbacks, shorthand and
  side-shorthand storage, and fail-closed unsupported fallback forms.
- `border_custom_properties_resolve_with_fallbacks` verifies inherited
  aliases, concrete complete-border fallback, invalid and cyclic fallback,
  CSS-wide reset, `none`, and physical side projection through computed
  width, style, and color values.
- Existing complete-border parsing, component projection, cascade,
  `revert-layer`, CSS-wide, and border composition tests remain green.

Focused commands:

```text
cargo fmt --all && RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked border_parser_accepts_custom_property_aliases_and_fallbacks -- --nocapture
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked border_custom_properties_resolve_with_fallbacks -- --nocapture
```

Observed focused results: parser 1 passed, 0 failed, 1,364 filtered out;
resolution 1 passed, 0 failed, 1,364 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,365 passed, 0 failed, 1 ignored, 1,364 filtered
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

Observed documentation gate result: 1,269 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.

The remaining issue #40 gates stay explicit: current-color and non-complete
fallback forms, nested variable grammar, registered properties, full CSS
variable grammar, and complete CSS/Web IDL parity.
