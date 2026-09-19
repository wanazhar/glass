# Native engine browser slice 626: flex alignment variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in
`justify-content`, `align-items`, `align-self`, and `align-content`. Standalone
`var(--name)` values and concrete keyword fallbacks are retained through the
existing local cascade and non-inherited alignment state.

Resolution covers inherited custom-property aliases, invalid and cyclic
fallback handling, reset and explicit inherit behavior, declaration-order
precedence, and existing `revert-layer` rollback. The bounded alignment
keyword grammar remains unchanged. Full custom-property grammar, registered
properties, and complete CSS and Web IDL parity remain issue #40 gates.

## Focused coverage

- `flex_alignment_parser_accepts_custom_property_aliases_and_fallbacks`
  verifies standalone aliases across all four alignment properties, concrete
  keyword fallbacks, declaration storage, and rejection of nested or
  unsupported fallback forms.
- `flex_alignment_custom_properties_resolve_with_fallbacks` verifies
  inherited aliases, non-inherited alignment state, concrete fallback outcomes,
  invalid and cyclic values, reset behavior, and declaration-order precedence.
- Existing `justify-content`, `align-items`, `align-self`, and `align-content`
  cascade, CSS-wide, explicit-inherit, and `revert-layer` tests remain green.

Focused commands:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked flex_alignment_parser_accepts_custom_property_aliases_and_fallbacks -- --nocapture
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked flex_alignment_custom_properties_resolve_with_fallbacks -- --nocapture
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked justify_content -- --nocapture
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked align_items -- --nocapture
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked align_self -- --nocapture
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked align_content -- --nocapture
```

Observed focused results: the parser and resolution tests each passed with
1,377 filtered tests; each of the four property suites passed 4 tests with
1,375 filtered.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,379 passed, 0 failed, 1 ignored, 1,378 filtered out.

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

Observed documentation gate result: 1,276 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.
