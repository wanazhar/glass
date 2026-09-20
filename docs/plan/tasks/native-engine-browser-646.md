# Native Engine Browser Slice 646: Typed Structural Selectors

Status: complete locally.

## Scope

Add bounded native support for typed structural pseudo-classes:

- `:first-of-type`
- `:last-of-type`
- `:only-of-type`

These pseudo-classes use the attached document's element siblings filtered by
local element name. The existing nth-position matcher supplies the shared
position semantics, and document-aware matching is used by selector locators
and stylesheet cascade.

Unsupported dynamic pseudo-classes and pseudo-elements remain fail-closed.

## Implementation

- Extended `NativePseudoClass` with the three typed structural variants.
- Parsed the pseudo-class names case-insensitively alongside the existing
  structural forms.
- Reused typed sibling position matching for first, last, and only positions.
- Added parser, document matching, and stylesheet-cascade coverage.

## Verification

- `cargo test -p glass-browser --lib of_type --locked -- --nocapture`
  - 1 passed; 0 failed; 1420 filtered out.
- `cargo test -p glass-browser --lib selector --locked -- --nocapture`
  - 16 passed; 0 failed; 1405 filtered out.
- `RUST_MIN_STACK=33554432 cargo test -p glass-browser --lib --locked`
  - 1421 passed; 0 failed; 1 ignored.
- `cargo fmt --all` completed successfully.

Package, workspace, documentation, and release gates passed after the slice
documentation was recorded:

- `scripts/check-rust-workspace.sh fast-check` plus locked builds for
  `glass-browser`, `glass-dev`, and locked metadata completed successfully.
- Documentation depth, coverage, release-truth, formatting, and diff checks
  completed successfully: 1296 Markdown files, 93 current guides, 19
  substantive contracts, 346 full-product MCP tools (101 browser-only), 17
  examples, 22 public modules, current documents=83, previous-version hits=63,
  semantic audit hits=1407, and zero current-claim failures.

Full selector grammar, case-sensitivity flags, namespaces, pseudo-elements,
complete CSS and Web IDL parity remain issue #40 gates.
