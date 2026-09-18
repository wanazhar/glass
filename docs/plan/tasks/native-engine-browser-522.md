# Native-engine browser slice 522: optical font sizing

Status: complete on the current source line.

## Objective

Carry inherited CSS `font-optical-sizing: auto|none` through the native
parser, cascade, computed-style/CSSOM projection, and text metrics. For
`auto`, supply the current bounded CSS font size to an advertised OpenType
`opsz` axis; `none` must suppress that automatic coordinate. Explicit
`font-variation-settings` and `@font-face`/script-created `FontFace`
variation descriptors retain priority over the automatic coordinate.

## Contract

- Parse only the case-insensitive `auto` and `none` values plus the existing
  inherited CSS-wide keywords; reject mixed or unknown tokens atomically.
- Preserve inheritance, `initial`/`unset`/`revert`/`revert-layer`, source order,
  importance, and the existing two-crate/content-process wire boundaries.
- Project computed values as canonical `auto` or `none` through CSSOM and
  `getPropertyValue()`.
- Add an automatic `opsz` coordinate only for an advertised face axis, using
  the bounded computed font size in the existing thousandths representation;
  authored element coordinates override face descriptors, and both override
  automatic optical sizing.
- Feed the effective coordinate to both HarfRust shaping and the existing
  variation-aware outline rasterizer without changing static-face behavior or
  `font-optical-sizing:none`.

## Tradeoffs and explicit boundary

This closes the next variable-font CSS control without a new dependency or a
third crate. The implementation does not claim `font-size-adjust`, optical
size interpolation outside an OpenType `opsz` axis, color glyph tables, WOFF2,
hinting, variable `STAT`/named-style UI, or complete FontFace/Web IDL parity;
those remain separate issue #40 gates.

## Verification

The final record will include scoped check, focused parser/cascade/CSSOM and
variable-axis tests, the affected `font_` group, the full locked
`glass-browser` library regression, and documentation inventory/depth/TUI
gates. No remote CI, push, release, tag, registry publication, or native/CDP
parity certification is implied by this local slice.

## Results

- `cargo fmt --all -- --check` and `git diff --check` passed.
- The final cached `cargo check --quiet -p glass-browser --lib --locked` gate
  passed in 0.27 seconds after formatting and documentation updates.
- The focused `font_optical_sizing` parser/cascade/CSSOM group passed 3/3
  tests in 111.47 seconds; the affected `font_` group passed 89/89 tests in
  29.43 seconds.
- The full locked `glass-browser` library gate passed 1,224 tests with 1
  ignored and 0 failures in 63.94 seconds under
  `RUST_MIN_STACK=8388608`.
- Documentation truth passed for 1,172 Markdown documents with 83 current
  documents, 63 previous-version hits, 1,349 semantic-audit hits, and 0
  current-claim failures. Documentation coverage passed with 346 full-product
  MCP tools (101 browser-only), 17 examples, and 22 public modules; depth
  passed with 93 current guides and 19 substantive contracts; TUI inventory
  passed with 15 implementation keys and 63 documentation markers.
- No remote CI, push, release, tag, registry publication, or native/CDP parity
  certification is claimed by this local slice.
