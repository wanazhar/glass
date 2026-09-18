# Native-engine browser slice 523: relative font weights

Status: complete on the current source line.

## Objective

Carry CSS `font-weight: lighter|bolder` through the native property parser and
cascade, resolving it against the inherited computed weight before font
selection, variable-axis mapping, paint selection, and CSSOM projection.
Absolute weights remain the only accepted `@font-face` and script-created
`FontFace` descriptor values.

## Contract

- Accept case-insensitive `lighter` and `bolder` only for the inherited CSS
  property; keep the existing absolute parser for `@font-face`, `FontFace`,
  and local-font source descriptors.
- Resolve relative values using the CSS Fonts relative-weight bands against
  the parent computed weight, including numeric weights from 1 through 1000.
- Preserve inheritance, `initial`/`unset`/`revert`/`revert-layer`, source order,
  importance, and the existing absolute computed `FontWeightValue` wire shape.
- Feed the resolved absolute weight through existing face scoring, advertised
  `wght` mapping, synthetic bold threshold, and CSSOM serialization.
- Keep descriptor parsing, script local-font lookup, static-face behavior, and
  the two-crate/content-process boundary unchanged.

## Tradeoffs and explicit boundary

This closes the remaining relative-weight grammar gap without introducing a
new dependency or widening serialized computed-style payloads. The slice does
not add numeric `@font-face` ranges, variable `STAT`/named-style UI, color
glyph tables, WOFF2, hinting, font-display timing, or complete FontFace/Web IDL
parity; those remain separate issue #40 gates.

## Verification

The final record will include scoped check, focused absolute/relative parser and
cascade tests, CSSOM coverage, the affected `font_` group, the full locked
`glass-browser` library regression, and documentation inventory/depth/TUI
gates. No remote CI, push, release, tag, registry publication, or native/CDP
parity certification is implied by this local slice.

## Results

- `cargo fmt --all -- --check` and `git diff --check` passed.
- The final `cargo check --quiet -p glass-browser --lib --locked` gate passed
  in 11.45 seconds after formatting and documentation updates.
- The focused `font_weight` group passed 7/7 tests in 36.41 seconds; the
  affected `font_` group passed 93/93 tests in 30.10 seconds.
- The full locked `glass-browser` library gate passed 1,228 tests with 1
  ignored and 0 failures in 64.08 seconds under
  `RUST_MIN_STACK=8388608`.
- Documentation truth passed for 1,173 Markdown documents with 83 current
  documents, 63 previous-version hits, 1,351 semantic-audit hits, and 0
  current-claim failures. Documentation coverage passed with 346 full-product
  MCP tools (101 browser-only), 17 examples, and 22 public modules; depth
  passed with 93 current guides and 19 substantive contracts; TUI inventory
  passed with 15 implementation keys and 63 documentation markers.
- No remote CI, push, release, tag, registry publication, or native/CDP parity
  certification is claimed by this local slice.
