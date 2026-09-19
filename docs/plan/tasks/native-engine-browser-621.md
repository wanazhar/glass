# Native engine browser slice 621: visibility variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in
`visibility`. Standalone `var(--name)` values and hidden or visible fallbacks
are retained through the existing local cascade and hidden-subtree owner.

Resolution covers inherited custom-property aliases, invalid and cyclic
fallback handling, and existing `revert-layer` rollback. The existing
hidden/visible grammar remains unchanged. CSS-wide values beyond
`revert-layer`, `collapse`, nested-variable, registered-property, full CSS
variable grammar, and complete CSS/Web IDL parity remain explicit issue #40
gates.

## Focused coverage

- `visibility_parser_accepts_custom_property_aliases_and_fallbacks` verifies
  standalone aliases, hidden and visible fallbacks, declaration storage,
  nested-variable rejection, and `revert-layer` parsing.
- `visibility_custom_properties_resolve_with_fallbacks` verifies inherited
  custom-property aliases, hidden and visible fallback outcomes, invalid and
  cyclic fallback handling, and computed hidden-subtree state.
- Existing display/visibility cascade, `revert-layer`, diagnostics, and
  hidden-subtree tests remain green.

Focused commands:

```text
cargo fmt --all && RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked visibility_parser_accepts_custom_property_aliases_and_fallbacks -- --nocapture
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked visibility_custom_properties_resolve_with_fallbacks -- --nocapture
```

Observed focused results: parser 1 passed, 0 failed, 1,368 filtered out;
resolution 1 passed, 0 failed, 1,368 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,369 passed, 0 failed, 1 ignored, 1,368 filtered
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

Observed documentation gate result: 1,271 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.

The remaining issue #40 gates stay explicit: CSS-wide values beyond
`revert-layer`, `collapse`, nested-variable grammar, registered properties,
full CSS variable grammar, and complete CSS/Web IDL parity.
