# Native engine browser slice 642: bounded functional selectors

Status: implementation, native-library, package, and documentation
verification complete locally. Scope: issue #40 native-browser completion.

## Change

The native CSS selector model now supports bounded functional pseudo-classes
`:not(...)`, `:is(...)`, and `:where(...)`. Each function accepts a bounded
selector list, including nested bounded functional pseudos. `:not()` negates
all alternatives, while `:is()` and `:where()` match any alternative;
`:where()` contributes zero specificity and the other functions contribute the
maximum argument specificity, preserving the existing bounded cascade order.

Selector-list parsing now tracks bracket, function, and quoted-attribute
boundaries. Stylesheet rule parsing uses the same top-level selector-list
scanner, so commas inside functional pseudos no longer split rules into
malformed fragments. Unsupported functional forms such as `:nth-child()` and
`:has()`, malformed arguments, pseudo-elements, and unsupported selector
syntax remain fail-closed.

Full CSS selector grammar, `:has()` relative selectors, nth/of-child
arithmetic, namespaces, pseudo-elements, selector caching, and complete CSS
and Web IDL parity remain issue #40 gates.

## Focused coverage

- `selector_parser_supports_bounded_pseudo_classes` verifies functional
  parsing, specificity for `:not`, `:is`, and `:where`, and fail-closed
  rejection of unsupported or malformed forms.
- `document_selector_matches_bounded_negation_pseudo_class` verifies positive
  and negative `:not()` matches against classes, tags, and nested selectors.
- `document_selector_matches_bounded_selector_list_pseudos` verifies `:is()`,
  `:where()`, and nested selector-list matching.
- `functional_pseudo_classes_apply_during_style_cascade` verifies functional
  selectors survive stylesheet selector-list parsing and affect computed color
  and background values.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib selector --locked -- --nocapture
```

Observed focused result: 12 passed, 0 failed, 1,402 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,414 passed, 0 failed, 1 ignored, 1,413 filtered out.

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

Observed documentation gate result: 1,292 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.
