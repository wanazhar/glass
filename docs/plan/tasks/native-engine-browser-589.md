# Native engine browser slice 589: font-variant shorthand variables

Status: completed locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now accepts bounded custom-property substitution in the
inherited `font-variant` shorthand. A standalone `var(--name)` or a
`var(--name, full-shorthand)` fallback is split into the six existing inherited
OpenType component declarations: ligatures, caps, position, alternates, East
Asian, and numeric controls.

When a component resolver reads a custom property, it accepts either the
component's direct grammar or a complete `font-variant` shorthand and projects
the relevant component. This preserves aliases, invalid-value fallback,
cyclic-value bounds, and CSS-wide custom-property mappings without introducing
a second cascade path. Direct shorthand parsing and component-specific
longhands remain unchanged.

Nested variable grammar and CSS-wide shorthand fallbacks that cannot be
represented as concrete component fallback values remain explicit issue #40
gates rather than silently accepted.

## Focused coverage

- `font_variant_shorthand_expands_existing_font_variant_controls` also verifies
  standalone and full-shorthand custom-property declaration parsing plus nested
  fallback rejection.
- `inherited_font_variant_shorthand_custom_properties_resolve_with_fallbacks`
  verifies aliases, full-shorthand fallback projection, invalid values, cycles,
  and CSS-wide `initial`/`inherit` custom-property mappings across all six
  component values.
- Existing shorthand cascade and FontFace projection tests remain green.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked font_variant_shorthand -- --nocapture
```

Observed focused result: 4 passed, 0 failed, 1,313 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,317 passed, 0 failed, 1 ignored, 1,316 filtered out.

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
contracts; 1,239 Markdown files; 346 full-product MCP tools (101 browser-only);
17 examples; 22 public modules; 83 current release documents; 63 previous
version references; 1,386 semantic audit hits; zero current-claim failures;
`cargo fmt --all -- --check`; and `git diff --check` all passed. This slice
keeps the remaining issue #40 gates explicit: nested variable grammar,
registered properties, full CSS variable grammar, CSS-wide shorthand
fallbacks, and complete CSS/Web IDL parity.
