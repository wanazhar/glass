# Native engine browser slice 645: attribute selector operators

Status: implementation, native-library, package, and documentation
verification complete locally. Scope: issue #40 native-browser completion.

## Change

The native CSS selector model now supports typed attribute selector operators:
presence (`[attr]`), exact equality (`=`), whitespace-token inclusion (`~=`),
language dash-match (`|=`), prefix (`^=`), suffix (`$=`), and substring (`*=`).
Quoted and unquoted bounded values retain the existing fail-closed validation;
operator values are normalized into one native attribute selector model.

Document-aware matching feeds both CSS action locators and stylesheet cascade.
Whitespace-token matching splits ASCII whitespace, dash-match accepts the exact
language or a language followed by `-`, and the prefix/suffix/substring forms
use bounded literal comparisons. Malformed operators, empty operator values,
invalid names, and unsupported value syntax remain fail-closed.

Full CSS selector grammar, case-sensitivity flags, namespaces, pseudo-elements,
selector caching, and complete CSS and Web IDL parity remain issue #40 gates.

## Focused coverage

- `selector_parser_supports_bounded_compound_and_descendant_selectors` verifies
  all bounded attribute operators and malformed operator rejection.
- `document_selector_matches_bounded_attribute_operators` verifies presence,
  exact, token, dash-match, prefix, suffix, substring, and negative matches.
- `attribute_selector_operators_apply_during_style_cascade` verifies operator
  selectors affect computed color and background values.

Focused commands:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib selector --locked -- --nocapture
cargo test -p glass-browser --lib attribute_selector --locked -- --nocapture
```

## Verification

Focused selector result:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib selector --locked -- --nocapture
```

Observed focused result: 16 passed, 0 failed, 1,404 filtered out.

Direct attribute-cascade result:

```text
cargo test -p glass-browser --lib attribute_selector --locked -- --nocapture
```

Observed direct result: 1 passed, 0 failed, 1,419 filtered out.

Full native-library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,420 passed, 0 failed, 1 ignored, 1,419 filtered out.

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

Observed documentation result: 1,295 Markdown documents; 93 current guides;
19 substantive contracts; 346 full-product MCP tools (101 browser-only); 17
examples; 22 public modules; 83 current documents; 63 previous-version hits;
1,407 semantic audit hits; zero current-claim failures. Formatting and diff
checks passed.
