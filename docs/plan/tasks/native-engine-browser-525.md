# Native-engine browser slice 525: WOFF2 font admission

Status: complete on the current source line.

## Objective

Make WOFF2 a first-class native font container for CSS and script-created
`FontFace` resources. Decode the compressed container to the existing bounded
SFNT font owners without changing the two-crate workspace or the current
fontdue/HarfRust rendering contract.

## Contract

- Admit valid WOFF2 payloads up to the existing 4 MiB per-font limit and reject
  malformed signatures, lengths, table counts, compressed sizes, decompressed
  streams, and outputs outside the native font budget before font parsing.
- Use a pure-Rust decoder and a bounded Brotli output sink; preserve the
  existing WOFF 1.0 normalizer, SFNT/TrueType/OpenType behavior, collection
  handling, and fail-closed resource admission.
- Carry WOFF2 through CSS `@font-face` and script-created `FontFace` source
  lists; `format("woff2")` must be recognized as a supported candidate and
  later candidates must remain available when a local or remote WOFF2 source
  fails.
- Keep the native content-process wire shape, font-family/style/stretch/weight
  selection, requested variable coordinates, and two-crate boundary unchanged.

## Tradeoffs and explicit boundary

The pure-Rust Wuff decoder avoids a C/C++ build toolchain and shares the
existing native font byte budget, but adds Brotli code and compile time to the
`glass-browser` native feature. This slice enables the WOFF2 container; it does
not claim color-glyph rendering, variable-axis completeness, hinting,
`font-display` timing, or complete FontFace/Web IDL parity.

## Verification

The final record will include the locked scoped check, a valid WPT WOFF2
normalization/admission test, the affected `font_` regression group, the full
locked `glass-browser` library regression, formatting/diff checks, and the
documentation inventory/depth/TUI gates. No remote CI, push, release, tag,
registry publication, or native/CDP parity certification is implied by this
local slice.

## Results

- The locked scoped `glass-browser` library check passed in 50.44 seconds.
- The valid WPT WOFF2 normalization and native-admission test passed: 1/1 in
  44.32 seconds.
- The affected `font_` regression group passed: 97/97 tests in 30.62 seconds.
- The full locked `glass-browser` library regression passed: 1,232 passed, 1
  ignored, 0 failed in 64.31 seconds (wall time 64.62 seconds), with
  `RUST_MIN_STACK=8388608`.
- Release-documentation truth passed: 1,175 Markdown files, 83 current
  documents, 63 previous-version hits, 1,355 semantic audit hits, and 0
  current-claim failures.
- Documentation coverage passed: 346 full-product MCP tools (101
  browser-only), 17 examples, and 22 public modules.
- Documentation depth passed: 93 current guides routed/audited and 19
  substantive contracts; the TUI shortcut inventory passed with 15
  implementation help keys and 63 documentation markers.
