# Native engine browser slice 604: overflow variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in the
`overflow`, `overflow-x`, and `overflow-y` properties. Standalone
`var(--name)` and `var(--name, overflow-keyword)` fallbacks are retained
through the axis cascade and resolved against the inherited overflow value
before clipping and computed-style projection.

Resolution preserves shorthand-to-axis expansion, explicit `inherit`, CSS-wide
`initial`/`unset`/`revert` resets, inherited aliases, invalid-value fallback,
and bounded cyclic-value failure. Existing `hidden`, `clip`, `auto`, `scroll`,
and `visible` values and axis precedence remain unchanged; unsupported nested
variable grammar remains fail-closed.

## Focused coverage

- `overflow_parser_accepts_custom_property_aliases_and_fallbacks` verifies
  direct values, standalone custom-property references, keyword fallbacks,
  nested-variable rejection, and CSS-wide declaration parsing.
- `overflow_custom_properties_resolve_with_fallbacks` verifies shorthand and
  axis aliases, keyword fallback, invalid values, cycles, and CSS-wide
  `initial`/`inherit` custom-property mappings through clipping projection.
- Existing axis, cascade, reset, revert-layer, diagnostic, and important
  overflow tests remain green.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked overflow -- --nocapture
```

Observed focused result: 14 passed, 0 failed, 1,321 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,334 passed, 0 failed, 1 ignored, 0 filtered out.

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
contracts; 1,254 Markdown files; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current release documents;
63 previous-version references; 1,401 semantic audit hits; zero current-claim
failures; `cargo fmt --all -- --check`; and `git diff --check` all passed.

The remaining issue #40 gates stay explicit: nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity.
