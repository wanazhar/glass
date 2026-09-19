# Native engine browser slice 643: bounded relative selectors

Status: implementation, native-library, package, and documentation
verification complete locally. Scope: issue #40 native-browser completion.

## Change

The native CSS selector model now supports bounded functional `:has(...)`
pseudo-classes. Relative arguments default to descendant matching and may start
with child (`>`), adjacent-sibling (`+`), or subsequent-sibling (`~`) relations.
Each argument retains a parsed selector chain, uses the maximum argument
specificity, and matches against the attached document rather than the local
node-only shortcut.

Bounded descendant traversal respects `MAX_NATIVE_DOM_DEPTH`; child and
following-sibling matching ignore non-element nodes as CSS relationships do.
Malformed relative arguments, unsupported selector syntax, and empty argument
lists remain fail-closed. Existing `:not(...)`, `:is(...)`, and `:where(...)`
parsing and stylesheet selector-list handling remain unchanged.

Full CSS selector grammar, nth/of-child arithmetic, namespaces,
pseudo-elements, selector caching, and complete CSS and Web IDL parity remain
issue #40 gates.

## Focused coverage

- `selector_parser_supports_bounded_pseudo_classes` verifies `:has(...)`
  parsing, specificity, explicit relative combinators, and malformed rejection.
- `document_selector_matches_bounded_has_pseudo_class` verifies descendant,
  child, nested-chain, adjacent-sibling, subsequent-sibling, and negative
  relative matches through the attached DOM.
- `has_pseudo_class_applies_during_style_cascade` verifies document-aware
  `:has(...)` matching affects computed styles.

Focused commands:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib selector --locked -- --nocapture
cargo test -p glass-browser --lib has_pseudo_class --locked -- --nocapture
```

## Verification

Focused selector result:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib selector --locked -- --nocapture
```

Observed focused result: 13 passed, 0 failed, 1,403 filtered out.

The two direct `:has(...)` tests also passed:

```text
cargo test -p glass-browser --lib has_pseudo_class --locked -- --nocapture
```

Observed direct result: 2 passed, 0 failed, 1,414 filtered out.

Full native-library command:

```text
RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked
```

Observed full result: 1,416 passed, 0 failed, 1 ignored, 1,415 filtered out.

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

Observed documentation result: 1,293 Markdown documents; 93 current guides;
19 substantive contracts; 346 full-product MCP tools (101 browser-only); 17
examples; 22 public modules; 83 current documents; 63 previous-version hits;
1,407 semantic audit hits; zero current-claim failures. Formatting and diff
checks passed.
