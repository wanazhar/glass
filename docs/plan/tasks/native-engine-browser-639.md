# Native engine browser slice 639: inline supports rules

Status: implementation, native-library, package, and documentation
verification complete locally. Scope: issue #40 native-browser completion.

## Change

The native CSS engine now parses bounded inline `@supports` blocks into
supports-qualified style rules. Parsed rules retain both media and supports
conditions when one conditional rule is nested inside the other; `@layer`
propagation preserves both conditions, while repeated nesting of the same
conditional kind is explicitly diagnosed.

The existing supports evaluator now gates both normal declarations and
custom-property candidates during cascade. Supported declaration conditions,
logical `and`/`or`/`not`, and media-plus-supports composition therefore remain
fail-closed for unsupported CSS declarations instead of leaking inactive rules.

Full CSS Conditional Rules grammar, nested conditional composition beyond the
bounded representation, complete CSS parity, and complete Web IDL parity remain
issue #40 gates.

## Focused coverage

- `inline_supports_rules_match_supported_declarations_and_composition` verifies
  supported and unsupported declarations plus media/supports composition at
  wide and narrow viewports.
- `inline_supports_rules_filter_custom_properties_by_support` verifies an
  unsupported condition cannot leak a custom property into an active `var()`
  declaration.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked inline_supports_rules -- --nocapture
```

Observed focused result: 2 passed, 0 failed, 1,403 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,405 passed, 0 failed, 1 ignored, 1,404 filtered out.

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

Observed documentation gate result: 1,289 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.
