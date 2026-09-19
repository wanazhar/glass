# Native engine browser slice 600: inherited letter-spacing variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in the
inherited `letter-spacing` property. A standalone `var(--name)` or a
`var(--name, positive-pixel-value)` fallback is retained through the cascade
and resolved against the inherited letter-spacing value before normal spacing
computation.

Resolution preserves inherited aliases, CSS-wide `initial`/`inherit` custom
property mappings, invalid-value fallback, and bounded cyclic-value failure.
Existing non-negative pixel letter-spacing values and computed-style projection
remain unchanged; unsupported nested variable grammar remains fail-closed.

## Focused coverage

- `inherited_text_spacing_declaration_parsers_accept_standalone_css_wide_keywords`
  also verifies standalone letter-spacing custom-property references,
  positive-pixel fallbacks, and rejection of nested variable fallback grammar.
- `inherited_letter_spacing_custom_properties_resolve_with_fallbacks` verifies
  inherited aliases, positive-pixel fallback, invalid values, cycles, and
  CSS-wide `initial`/`inherit` custom-property mappings.
- Existing letter-spacing cascade, inheritance, layout, and invalid-value tests
  remain green.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked letter_spacing -- --nocapture
```

Observed focused result: 2 passed, 0 failed, 1,326 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,328 passed, 0 failed, 1 ignored, 1,327 filtered out.

## Package and documentation gates

Package gate:

```text
scripts/check-rust-workspace.sh fast-check && cargo build -p glass-browser --bin glass-browser --locked && cargo build -p glass-dev --bin glass --locked && cargo metadata --no-deps --format-version 1 --locked >/dev/null
```

Observed package gate result: `check-rust-workspace.sh fast-check`,
`glass-browser` binary build, `glass-dev` binary build, and locked metadata
validation all passed.

Documentation gate:

```text
python3 scripts/check-documentation-depth.py && python3 scripts/check-documentation-coverage.py && python3 scripts/check-release-documentation.py && cargo fmt --all -- --check && git diff --check
```

Observed documentation gate result: 93 current guides and 19 substantive
contracts; 1,250 Markdown files; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current release documents;
63 previous-version references; 1,398 semantic audit hits; zero current-claim
failures; `cargo fmt --all -- --check`; and `git diff --check` all passed.

The remaining issue #40 gates stay explicit: nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity.
