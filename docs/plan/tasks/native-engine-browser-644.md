# Native engine browser slice 644: bounded nth selectors

Status: implementation, native-library, package, and documentation
verification complete locally. Scope: issue #40 native-browser completion.

## Change

The native CSS selector model now supports bounded arithmetic position
pseudo-classes: `:nth-child()`, `:nth-last-child()`, `:nth-of-type()`, and
`:nth-last-of-type()`. Formula parsing accepts integer positions,
`odd`/`even`, and bounded `an+b` forms after removing allowed ASCII
whitespace. Sibling matching counts element children and, for `of-type`
forms, only siblings with the candidate element name.

The matcher uses attached-document relationships for both CSS action locators
and stylesheet cascade. Negative coefficients, zero coefficients, reverse
positions, and formulas with no matching positive index follow the arithmetic
position rules. Malformed formulas, unsupported `of` clauses, and unsupported
selector syntax remain fail-closed.

Full CSS selector grammar, selector namespaces, pseudo-elements, selector
caching, and complete CSS and Web IDL parity remain issue #40 gates.

## Focused coverage

- `selector_parser_supports_bounded_pseudo_classes` verifies integer,
  `an+b`, and `odd` parsing with pseudo-class specificity, plus malformed and
  unsupported formula rejection.
- `document_selector_matches_bounded_nth_pseudo_classes` verifies forward,
  reverse, `of-type`, odd, negative-coefficient, and sibling-position matches.
- `nth_pseudo_classes_apply_during_style_cascade` verifies arithmetic selectors
  affect computed color and background values.

Focused commands:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib selector --locked -- --nocapture
cargo test -p glass-browser --lib nth --locked -- --nocapture
```

## Verification

Focused selector result:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib selector --locked -- --nocapture
```

Observed focused result: 14 passed, 0 failed, 1,404 filtered out.

Direct nth behavior result:

```text
cargo test -p glass-browser --lib nth_pseudo --locked -- --nocapture
```

Observed direct result: 2 passed, 0 failed, 1,416 filtered out.

Full native-library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,418 passed, 0 failed, 1 ignored, 1,417 filtered out.

Package gate:

```text
scripts/check-rust-workspace.sh fast-check && cargo build -p glass-browser --bin glass-browser --locked && cargo build -p glass-dev --bin glass --locked && cargo metadata --no-deps --format-version 1 --locked >/dev/null
```

Observed package result: the fast workspace check, locked `glass-browser` and
`glass-dev` binary builds, and locked metadata validation all passed without
warnings.

Documentation gate:

```text
python3 scripts/check-documentation-depth.py && python3 scripts/check-documentation-coverage.py && python3 scripts/check-release-documentation.py && cargo fmt --all -- --check && git diff --check
```

Observed documentation result: 1,294 Markdown documents; 93 current guides;
19 substantive contracts; 346 full-product MCP tools (101 browser-only); 17
examples; 22 public modules; 83 current documents; 63 previous-version hits;
1,407 semantic audit hits; zero current-claim failures. Formatting and diff
checks passed.
