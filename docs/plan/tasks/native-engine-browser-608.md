# Native engine browser slice 608: z-index variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in
`z-index`. Standalone `var(--name)` and integer or `auto`
`var(--name, value)` fallbacks are retained through the local cascade and
resolved against inherited custom properties before computed-style projection.

Resolution preserves integer bounds, `auto` mapping, local cascade
precedence, CSS-wide reset mappings, `revert-layer` rollback, invalid-value
fallback, and bounded cyclic-value failure. Unsupported nested variable
grammar remains fail-closed.

## Focused coverage

- `z_index_parser_accepts_custom_property_aliases_and_fallbacks` verifies
  bounded integer values, standalone custom-property references, negative
  integer fallbacks, CSS-wide `initial`/`inherit` parsing, and nested-variable
  rejection.
- `z_index_custom_properties_resolve_with_fallbacks` verifies inherited
  aliases, negative integer fallback, invalid custom-property fallback, cyclic
  custom-property fallback, and CSS-wide `initial` reset mapping through
  computed-style projection.
- Existing z-index bounds, cascade, reset, revert-layer, and declaration
  parsing tests remain green.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked z_index -- --nocapture
```

Observed focused result: 2 passed, 0 failed, 1,341 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,343 passed, 0 failed, 1 ignored, 0 filtered out.

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
contracts; 1,258 Markdown files; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current release documents;
63 previous-version references; 1,406 semantic audit hits; zero current-claim
failures; `cargo fmt --all -- --check`; and `git diff --check` all passed.

The remaining issue #40 gates stay explicit: nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity.
