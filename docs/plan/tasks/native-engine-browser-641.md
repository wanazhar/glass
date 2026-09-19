# Native engine browser slice 641: bounded selector combinators

Status: implementation, native-library, package, and documentation
verification complete locally. Scope: issue #40 native-browser completion.

## Change

The native CSS selector model now retains the bounded combinator between each
compound selector. Descendant matching remains supported, and child (`>`),
adjacent-sibling (`+`), and subsequent-sibling (`~`) combinators now resolve
against the attached native document tree. Matching advances from each found
relationship, so mixed descendant, child, and sibling chains preserve selector
semantics and pseudo-class evaluation for every compound.

The parser accepts explicit combinators with or without surrounding whitespace,
keeps combinator characters inside attribute values, and rejects leading,
trailing, or repeated combinators. The existing bounded compound-count and
specificity limits remain in force. Unsupported selector grammar remains
fail-closed rather than silently becoming a descendant selector.

Full CSS selector grammar, namespaces, functional selectors, pseudo-elements,
selector-list grammar, selector caching, and complete CSS and Web IDL parity
remain issue #40 gates.

## Focused coverage

- `selector_parser_supports_bounded_compound_and_descendant_selectors` verifies
  child, adjacent-sibling, and subsequent-sibling parsing plus malformed
  combinator rejection.
- `document_selector_matches_bounded_child_combinators` verifies direct-child
  versus descendant matching and applies a child-plus-adjacent selector through
  the stylesheet cascade.
- `document_selector_matches_bounded_sibling_combinators` verifies adjacent
  and subsequent sibling traversal in document order.

Focused command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib selector --locked -- --nocapture
```

Observed focused result: 10 passed, 0 failed, 1,401 filtered out.

Full native library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,411 passed, 0 failed, 1 ignored, 1,410 filtered out.

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

Observed documentation gate result: 1,291 Markdown documents; 93 current
guides; 19 substantive contracts; 346 full-product MCP tools (101
browser-only); 17 examples; 22 public modules; 83 current documents; 63
previous-version hits; 1,406 semantic audit hits; zero current-claim failures.
Formatting and diff checks passed.
