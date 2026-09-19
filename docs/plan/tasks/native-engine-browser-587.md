# Native engine browser slice 587: inherited font-variant-east-asian variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now stores inherited `font-variant-east-asian`
declarations as bounded declaration values instead of immediately discarding
custom-property references. Direct compound values, standalone `var(--name)`,
and compound-keyword `var(--name, value)` fallbacks are resolved through the
existing bounded custom-property recursion guard. The `font-variant` shorthand
maps its East Asian component through the same declaration wrapper, while
`inherit`, `initial`, `unset`, `revert`, and `revert-layer` retain their
existing inherited semantics.

Invalid custom-property values, CSS-wide custom-property values, unresolved
names, and cyclic references fail closed or select the supplied compound
fallback. Nested fallback grammar remains intentionally unsupported and is
covered as an explicit issue #40 gate rather than silently accepted.

## Focused coverage

- `font_variant_east_asian_parser_keeps_feature_groups_exclusive` also
  verifies standalone and compound fallback custom-property parsing plus
  nested-fallback rejection.
- `inherited_font_variant_east_asian_custom_properties_resolve_with_fallbacks`
  verifies inherited aliases, compound fallbacks, invalid values, cycles, and
  CSS-wide custom-property mappings through computed style.
- Existing East Asian mapping, inheritance/reset, shorthand, and FontFace
  computed-style tests remain green.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked font_variant_east_asian -- --nocapture
```

Observed focused result: 5 passed, 0 failed, 1,310 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,315 passed, 0 failed, 1 ignored, 1,314 filtered out.

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
contracts; 1,237 Markdown files; 346 full-product MCP tools (101 browser-only);
17 examples; 22 public modules; 83 current release documents; 63 previous
version references; 1,384 semantic audit hits; zero current-claim failures;
`cargo fmt --all -- --check`; and `git diff --check` all passed. This slice
keeps the remaining issue #40 gates explicit: complete shorthand substitution,
nested variable grammar, registered properties, full CSS variable grammar, and
complete CSS/Web IDL parity.
