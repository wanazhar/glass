# Native engine browser slice 635: grid template variables

Status: implementation and full native-library verification complete locally;
final package and documentation gates pending. Scope: issue #40
native-browser completion.

## Change

The native CSS engine now models bounded custom-property substitution for
`grid-template-columns` and `grid-template-rows`. Standalone `var(--name)`
values and concrete track-list fallbacks resolve through the existing grid
track grammar, including bounded lengths, `fr`, `auto`, and `repeat()`
expansion.

Resolution preserves inherited aliases, invalid and cyclic custom-property
handling, concrete fallback parsing, `none`, declaration-order precedence,
and the existing non-inherited grid reset behavior. Full grid grammar,
registered properties, implicit tracks, subgrid, and complete CSS and Web IDL
parity remain issue #40 gates.

## Focused coverage

- `grid_track_list_parser_accepts_custom_property_aliases_and_fallbacks`
  verifies standalone aliases, repeat/track-list fallbacks, and rejection of
  nested-variable or CSS-wide fallbacks.
- `grid_track_list_custom_properties_resolve_with_fallbacks` verifies direct
  values, inherited aliases, concrete fallbacks, invalid and cyclic custom
  properties, `none`, and declaration-order precedence for both axes.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked grid_track_list -- --nocapture
```

Observed focused result: 2 passed, 0 failed, 1,395 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
Observed full result: 1,396 passed, 0 failed, 1 ignored, 0 filtered out.


## Package and documentation gates

Package gate:

```text
scripts/check-rust-workspace.sh fast-check && cargo build -p glass-browser --bin glass-browser --locked && cargo build -p glass-dev --bin glass --locked && cargo metadata --no-deps --format-version 1 --locked >/dev/null
```

Documentation gate:

```text
python3 scripts/check-documentation-depth.py && python3 scripts/check-documentation-coverage.py && python3 scripts/check-release-documentation.py && cargo fmt --all -- --check && git diff --check
```

Observed package gate result: the fast workspace check, locked
`glass-browser` and `glass-dev` binary builds, and locked metadata validation
all passed without warnings.

Observed documentation gate result: 1,285 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.
