# Native engine browser slice 605: opacity variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in
`opacity`. Standalone `var(--name)` and percentage or numeric
`var(--name, value)` fallbacks are retained through the local cascade and
resolved against inherited custom properties before computed-style projection.

Resolution preserves opacity quantization, local cascade precedence, invalid
value fallback, and bounded cyclic-value failure. Existing direct opacity
parsing remains unchanged, including fail-closed handling for unsupported
CSS-wide forms; nested variable grammar remains unsupported and fail-closed.

## Focused coverage

- `opacity_parser_accepts_custom_property_aliases_and_fallbacks` verifies
  direct values, standalone custom-property references, percentage fallbacks,
  nested-variable rejection, and invalid direct declarations.
- `opacity_custom_properties_resolve_with_fallbacks` verifies inherited
  aliases, percentage fallback, invalid custom-property fallback, cyclic
  custom-property fallback, and computed-style quantization.
- Existing opacity cascade, revert-layer, parser, diagnostic, and raster tests
  remain green.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked opacity -- --nocapture
```

Observed focused result: 7 passed, 0 failed, 1,330 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,337 passed, 0 failed, 1 ignored, 0 filtered out.

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
contracts; 1,255 Markdown files; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current release documents;
63 previous-version references; 1,403 semantic audit hits; zero current-claim
failures; `cargo fmt --all -- --check`; and `git diff --check` all passed.

The remaining issue #40 gates stay explicit: nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity.
