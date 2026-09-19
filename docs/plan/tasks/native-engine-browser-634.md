# Native engine browser slice 634: background shorthand variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now models custom-property substitution in the full
`background` shorthand. Standalone `var(--name)` values and concrete shorthand
fallbacks expand through color, image, repeat, position, and size longhands
while retaining the existing cascade and reset behavior.

Resolution covers inherited aliases, invalid and cyclic custom properties,
concrete fallback parsing, and same-property declaration-order precedence.
Full background grammar, registered properties, and complete CSS and Web IDL
parity remain issue #40 gates.

## Focused coverage

- `background_shorthand_parser_accepts_custom_property_aliases_and_fallbacks`
  verifies standalone aliases, complete shorthand fallbacks, and rejection of
  nested-variable or CSS-wide fallbacks.
- `background_shorthand_custom_properties_resolve_all_components` verifies
  inherited aliases, image/color/repeat/position/size projection, invalid and
  cyclic fallback handling, and declaration-order precedence.

Focused commands:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked background_shorthand_parser_accepts_custom_property_aliases_and_fallbacks -- --nocapture
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked background_shorthand_custom_properties_resolve_all_components -- --nocapture
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked background_shorthand -- --nocapture
```

Observed focused results: the parser and resolution tests each passed; the
full `background_shorthand` filter passed both tests.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```
Observed full result: 1,394 passed, 0 failed, 1 ignored, 0 filtered out.

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

Observed documentation gate result: 1,284 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.
