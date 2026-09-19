# Native engine browser slice 602: text-overflow variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in the
local `text-overflow` property. A standalone `var(--name)` or a
`var(--name, keyword)` fallback is retained through the cascade and resolved
against the local text-overflow value before computed-style projection.

Resolution preserves local `inherit`, CSS-wide `initial`/`unset`/`revert`
resets, inherited aliases, invalid-value fallback, and bounded cyclic-value
failure. Existing `clip` and `ellipsis` values and local cascade precedence
remain unchanged; unsupported nested variable grammar remains fail-closed.

## Focused coverage

- `text_overflow_parser_accepts_clip_ellipsis_and_custom_properties` verifies
  direct values, standalone custom-property references, keyword fallbacks,
  nested-variable rejection, and CSS-wide declaration parsing.
- `text_overflow_custom_properties_resolve_with_fallbacks` verifies inherited
  aliases, keyword fallback, invalid values, cycles, and CSS-wide
  `initial`/`inherit` custom-property mappings through computed style.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked text_overflow -- --nocapture
```

Observed focused result: 4 passed, 0 failed, 1,327 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,330 passed, 0 failed, 1 ignored, 0 filtered out.

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
contracts; 1,252 Markdown files; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current release documents;
63 previous-version references; 1,399 semantic audit hits; zero current-claim
failures; `cargo fmt --all -- --check`; and `git diff --check` all passed.

The remaining issue #40 gates stay explicit: nested variable grammar,
registered properties, full CSS variable grammar, and complete CSS/Web IDL
parity.
