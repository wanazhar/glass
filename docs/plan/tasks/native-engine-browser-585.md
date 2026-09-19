# Native engine browser slice 585: inherited font-variant-position variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now stores inherited `font-variant-position`
declarations as bounded declaration values instead of immediately discarding
custom-property references. Direct values, standalone `var(--name)`, and
keyword `var(--name, value)` fallbacks are resolved through the existing
bounded custom-property recursion guard. The `font-variant` shorthand maps its
position component through the same declaration wrapper, while `inherit`,
`initial`, `unset`, `revert`, and `revert-layer` retain their existing
inherited semantics.

Invalid custom-property values, CSS-wide custom-property values, unresolved
names, and cyclic references fail closed or select the supplied keyword
fallback. Nested fallback grammar remains intentionally unsupported and is
covered as an explicit issue #40 gate rather than silently accepted.

## Focused coverage

- `font_variant_position_parser_accepts_only_supported_keywords` also verifies
  standalone and fallback custom-property parsing plus nested-fallback rejection.
- `inherited_font_variant_position_custom_properties_resolve_with_fallbacks`
  verifies inherited aliases, fallback values, invalid values, cycles, and
  CSS-wide custom-property mappings through computed style.
- Existing position mapping, inheritance/reset, shorthand, and FontFace
  computed-style tests remain green.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked font_variant_position -- --nocapture
```

Observed focused result: 6 passed, 0 failed, 1,307 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,313 passed, 0 failed, 1 ignored, 1,312 filtered out.

## Package and documentation gates

Package gate:

```text
scripts/check-rust-workspace.sh fast-check && cargo build -p glass-browser --bin glass-browser --locked && cargo build -p glass-dev --bin glass --locked && cargo metadata --no-deps --format-version 1 --locked >/dev/null
```

Documentation gate:

```text
python3 scripts/check-documentation-depth.py && python3 scripts/check-documentation-coverage.py && python3 scripts/check-release-documentation.py && cargo fmt --all -- --check && git diff --check
```

Observed package gate result: `check-rust-workspace.sh fast-check`,
`glass-browser` binary build, `glass-dev` binary build, and locked metadata
validation all passed.

Observed documentation gate result: 93 current guides and 19 substantive
contracts; 1,235 Markdown files; 346 full-product MCP tools (101 browser-only);
17 examples; 22 public modules; 83 current release documents; 63 previous
version references; 1,382 semantic audit hits; zero current-claim failures;
`cargo fmt --all -- --check`; and `git diff --check` all passed. This slice
keeps the remaining issue #40 gates explicit: complete shorthand substitution,
nested variable grammar, registered properties, full CSS variable grammar, and
complete CSS/Web IDL parity.
