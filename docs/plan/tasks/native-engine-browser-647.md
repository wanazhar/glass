# Native Engine Browser Slice 647: Attribute Case Flags

Status: implementation, native-library, package, and documentation verification
complete locally.

## Scope

Add bounded CSS attribute value case-sensitivity flags:

- `i`: ASCII-insensitive value matching.
- `s`: explicit case-sensitive value matching.

The flags apply to exact, whitespace-token (`~=`), language dash-match (`|=`),
prefix (`^=`), suffix (`$=`), and substring (`*=`) operators. Attribute names
continue to use the existing native HTML-compatible case-insensitive lookup.

Malformed, unknown, or unseparated modifiers remain fail-closed. Default
matching remains case-sensitive in this bounded native model; enumerated HTML
attribute-value defaults and complete Selectors grammar remain outside the
slice.

## Implementation

- Added a typed case-sensitivity mode to native attribute selectors.
- Parsed one trailing, whitespace-separated `i` or `s` modifier after quoted
  or unquoted attribute values.
- Applied ASCII-insensitive comparison consistently across all bounded
  attribute operators without allocating normalized copies.
- Added parser, document matching, and stylesheet-cascade coverage.

## Verification

- `cargo test -p glass-browser --lib case_flags --locked -- --nocapture`
  - 2 passed; 0 failed; 1421 filtered out.
- `cargo test -p glass-browser --lib selector_parser_supports_bounded_compound_and_descendant_selectors --locked -- --nocapture`
  - 1 passed; 0 failed; 1422 filtered out.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib selector --locked -- --nocapture`
  - 18 passed; 0 failed; 1405 filtered out.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`
  - 1423 passed; 0 failed; 1 ignored.
- `cargo check -p glass-browser --lib --locked` and `cargo fmt --all` completed
  successfully.

Package, workspace, documentation, and release gates passed after the slice's
documentation was recorded:

- `scripts/check-rust-workspace.sh fast-check` plus locked builds for
  `glass-browser`, `glass-dev`, and locked metadata completed successfully.
- Documentation depth, coverage, release-truth, formatting, and diff checks
  completed successfully: 1297 Markdown files, 93 current guides, 19
  substantive contracts, 346 full-product MCP tools (101 browser-only), 17
  examples, 22 public modules, current documents=83, previous-version hits=63,
  semantic audit hits=1407, and zero current-claim failures.
