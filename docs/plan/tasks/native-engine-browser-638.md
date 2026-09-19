# Native engine browser slice 638: inline media rules

Status: implementation, native-library, package, and documentation
verification complete locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now parses bounded inline `@media` blocks into
viewport-qualified style rules instead of treating the at-rule header as a
selector. Media metadata survives stylesheet construction so viewport changes
can activate or deactivate the same parsed rules. `@layer` blocks preserve the
active media condition, and nested `@media` blocks remain explicitly rejected
rather than silently broadening scope.

Cascade declarations and custom-property candidates both filter through the
existing bounded media-query evaluator. Supported conditions therefore include
the existing `screen`/`all`, width and height ranges, orientation,
`prefers-color-scheme: light`, bare `color`, comma-separated alternatives, and
`not`/`only` prefixes. Unsupported media features remain inactive.

Full Media Queries grammar, nested conditional-rule composition, complete CSS
parity, and complete Web IDL parity remain issue #40 gates.

## Focused coverage

- `inline_media_rules_match_bounded_viewport_features` verifies wide, narrow,
  compact, and non-screen media cascade behavior.
- `inline_media_rules_filter_custom_properties_by_viewport` verifies that an
  inactive media rule cannot leak a custom property into an active declaration.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked inline_media_rules -- --nocapture
```

Observed focused result: 2 passed, 0 failed, 1,401 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,403 passed, 0 failed, 1 ignored, 0 filtered out.

Package gate:

```text
scripts/check-rust-workspace.sh fast-check && cargo build -p glass-browser --bin glass-browser --locked && cargo build -p glass-dev --bin glass --locked && cargo metadata --no-deps --format-version 1 --locked >/dev/null
```

Observed package gate result: the fast workspace check, locked
`glass-browser` and `glass-dev` binary builds, and locked metadata validation
all passed without warnings.

Documentation gate:

```text
python3 scripts/check-documentation-depth.py && python3 scripts/check-documentation-coverage.py && python3 scripts/check-release-documentation.py && cargo fmt --all -- --check && git diff --check
```

Observed documentation gate result: 1,288 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.
