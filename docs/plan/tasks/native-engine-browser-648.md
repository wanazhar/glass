# Native Engine Browser Slice 648: Namespace Type Selectors

Status: implementation, native-library, package, and documentation verification
complete locally.

## Scope

Add bounded namespace-qualified type selectors for attached native DOM
namespace URIs:

- `svg|name`, `html|name`, and `math|name` built-in prefixes;
- `*|name` wildcard namespace;
- `|name` no-namespace matching;
- qualified universal forms such as `svg|*`.

Unqualified type selectors retain the existing native behavior. Unknown
prefixes, missing local names, and malformed namespace forms remain fail-closed.
The slice does not add `@namespace` declarations or attribute namespace
selectors.

## Implementation

- Added a typed namespace model matching HTML, SVG, MathML, wildcard, and
  no-namespace element URIs.
- Parsed namespace-qualified type names and universal local names while
  preserving existing selector specificity and compound parsing.
- Applied namespace checks to document-aware selector matching used by CSS
  action locators and stylesheet cascade.
- Added parser, attached-DOM matching, and stylesheet-cascade coverage.

## Verification
- `cargo test -p glass-browser --lib namespace --locked -- --nocapture`
  - 5 passed; 0 failed; 1421 filtered out.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib selector --locked -- --nocapture`
  - 21 passed; 0 failed; 1405 filtered out.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`
  - 1426 passed; 0 failed; 1 ignored.
- `cargo fmt --all` completed successfully.

Package, workspace, documentation, and release gates passed after this slice's
documentation was recorded:

- `scripts/check-rust-workspace.sh fast-check` plus locked builds for
  `glass-browser`, `glass-dev`, and locked metadata completed successfully.
- Documentation depth, coverage, release-truth, formatting, and diff checks
  completed successfully: 1298 Markdown files, 93 current guides, 19
  substantive contracts, 346 full-product MCP tools (101 browser-only), 17
  examples, 22 public modules, current documents=83, previous-version hits=63,
  semantic audit hits=1407, and zero current-claim failures.

Full namespace declaration and attribute-namespace grammar, selector caching,
complete CSS parity, and complete Web IDL parity remain issue #40 gates.
