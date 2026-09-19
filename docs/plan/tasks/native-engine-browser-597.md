# Native engine browser slice 597: inherited word-break variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in the
inherited `word-break` property. A standalone `var(--name)` or a
`var(--name, keyword)` fallback is retained through the cascade and resolved
against the inherited word-break value before normal word-break computation.

Resolution preserves inherited aliases, CSS-wide `initial`/`inherit` custom
property mappings, invalid-value fallback, and bounded cyclic-value failure.
Existing supported word-break values and computed-style projection remain
unchanged; unsupported nested variable grammar remains fail-closed.

## Focused coverage

- `word_break_parser_accepts_only_normal_and_break_all` also verifies
  standalone custom-property references, keyword fallbacks, and rejection of
  nested variable fallback grammar.
- `inherited_word_break_custom_properties_resolve_with_fallbacks` verifies
  inherited aliases, keyword fallback, invalid values, cycles, and CSS-wide
  `initial`/`inherit` custom-property mappings.
- Existing word-break cascade, inheritance, and invalid-value tests remain
  green.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked word_break -- --nocapture
```

Observed focused result: 3 passed, 0 failed, 1,322 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,325 passed, 0 failed, 1 ignored, 1,324 filtered out.

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
contracts; 1,247 Markdown files; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current release documents;
63 previous-version references; 1,395 semantic audit hits; zero current-claim
failures; `cargo fmt --all -- --check`; and `git diff --check` all passed.

The remaining issue #40 gates stay explicit: nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity.
