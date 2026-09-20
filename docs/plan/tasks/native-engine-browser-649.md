# Native Engine Browser Slice 649: Attribute Namespace Selectors

Status: implementation, native-library, package, and documentation verification
complete locally.

## Scope

Add bounded namespace-qualified attribute selectors backed by retained native
DOM attribute namespace metadata:

- built-in `xlink`, `xml`, `xmlns`, `svg`, `html`, and `math` prefixes;
- `*|name` wildcard namespace;
- `|name` no-namespace matching.

Unqualified attribute selectors retain their existing raw-name behavior. The
slice does not add `@namespace` declaration composition or a general namespace
map. Unknown prefixes, missing local names, and malformed forms remain
fail-closed.

## Implementation

- Added a typed attribute namespace selector model distinct from element
  namespace selectors and from unqualified attribute matching.
- Exposed bounded local-name and namespace-aware attribute lookup on native DOM
  nodes without allocating normalized attribute copies.
- Parsed namespace-qualified attribute names without confusing the existing
  language dash-match (`|=`) operator.
- Applied namespace-aware values to CSS action locators and stylesheet cascade.
- Added parser, attached-DOM matching, and stylesheet-cascade coverage.

## Verification
- `cargo test -p glass-browser --lib attribute_namespace --locked -- --nocapture`
  - 3 passed; 0 failed; 1426 filtered out.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib selector --locked -- --nocapture`
  - 24 passed; 0 failed; 1405 filtered out.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`
  - 1429 passed; 0 failed; 1 ignored.
- `cargo check -p glass-browser --lib --locked` and `cargo fmt --all` completed
  successfully.

Package, workspace, documentation, and release gates passed after this slice's
documentation was recorded:

- `scripts/check-rust-workspace.sh fast-check` plus locked builds for
  `glass-browser`, `glass-dev`, and locked metadata completed successfully.
- Documentation depth, coverage, release-truth, formatting, and diff checks
  completed successfully: 1299 Markdown files, 93 current guides, 19
  substantive contracts, 346 full-product MCP tools (101 browser-only), 17
  examples, 22 public modules, current documents=83, previous-version hits=63,
  semantic audit hits=1407, and zero current-claim failures.

Full `@namespace` declaration composition, complete attribute namespace
grammar, selector caching, complete CSS parity, and complete Web IDL parity
remain issue #40 gates.
