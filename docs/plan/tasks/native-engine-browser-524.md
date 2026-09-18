# Native-engine browser slice 524: numeric font-face weight ranges

Status: complete on the current source line.

## Objective

Carry absolute numeric `font-weight` ranges through CSS `@font-face`,
script-created `FontFace`, native font resources, and the content-process wire.
Use the range for face matching while retaining the requested element weight
for existing variable-axis mapping and paint behavior.

## Contract

- Parse one or two absolute `font-weight` values from 1 through 1000 for
  `@font-face` and `FontFace` descriptors; reject relative keywords, malformed
  lists, reversed ranges, zero, and values above 1000.
- Preserve the current singleton representation and descriptor formatting for
  `normal`, `bold`, and numeric values; format a true range as canonical
  `min max` values.
- Carry the bounded range through CSS resource admission, script install,
  content-process serialization, and restored font books. Older wire payloads
  without the optional range field must remain readable as singleton faces.
- Score a requested element weight at zero inside a descriptor range and by
  distance outside it; retain CSS family/style/stretch/unicode ordering and
  existing requested-weight `wght` variation mapping.
- Keep the computed CSS `font-weight` property, relative-weight resolver,
  static-face behavior, and the two-crate boundary unchanged.

## Tradeoffs and explicit boundary

This closes numeric `@font-face` range selection without adding dependencies or
changing the computed-style wire shape. It does not interpolate a variable
axis across a descriptor range, add variable `STAT`/named-style UI, color glyph
tables, WOFF2, hinting, font-display timing, or complete FontFace/Web IDL
parity; those remain separate issue #40 gates.

## Verification

The final record will include scoped check, focused range parser/descriptor,
wire-compatibility, and face-selection tests, the affected `font_` group, the
full locked `glass-browser` library regression, and documentation
inventory/depth/TUI gates. No remote CI, push, release, tag, registry
publication, or native/CDP parity certification is implied by this local
slice.

## Results

- The scoped `glass-browser` library check passed after the final source and
  documentation update in 15.31 seconds.
- The focused font-face range/parser/wire/selection batch passed: 33/33 tests
  in 130.58 seconds.
- The affected `font_` regression group passed: 96/96 tests in 30.69 seconds.
- The full locked `glass-browser` library regression passed: 1,231 passed, 1
  ignored, 0 failed in 63.80 seconds (wall time 64.10 seconds), with
  `RUST_MIN_STACK=8388608`.
- Release-documentation truth passed: 1,174 Markdown files, 83 current
  documents, 63 previous-version hits, 1,353 semantic audit hits, and 0
  current-claim failures.
- Documentation coverage passed: 346 full-product MCP tools (101
  browser-only), 17 examples, and 22 public modules.
- Documentation depth passed: 93 current guides routed/audited and 19
  substantive contracts; the TUI shortcut inventory passed with 15
  implementation help keys and 63 documentation markers.
