# Native engine browser slice 502: admit WOFF font sources

Status: completed locally on the current source line.

## Objective

Make the native font owner consume bounded WOFF 1.0 web-font resources instead
of treating their container header as a raw SFNT. This closes the ordinary WOFF
source path for CSS `@font-face` and script-created `FontFace` resources while
keeping unsupported WOFF2, variable, color, EOT, and SVG formats fail-closed.

## Contract

- Recognize WOFF 1.0 containers and validate their declared length, SFNT
  flavor, table count, table ranges, metadata/private ranges, and table
  non-overlap before allocation.
- Decode both uncompressed tables and zlib-compressed tables with a bounded
  expansion budget.
- Reconstruct a sorted SFNT directory with the original table checksums and
  repair the `head.checkSumAdjustment` field.
- Enforce the existing 4 MiB native font payload limit for both WOFF input and
  reconstructed SFNT output, plus bounded table count and aggregate table
  bytes.
- Normalize WOFF bytes before `fontdue` raster and HarfRust shaping admission
  for document resources and script-created `FontFace` installs.
- Advertise only formats the native parser actually admits: WOFF, TrueType,
  OpenType, and collection; WOFF2 and variable/color technologies remain
  unsupported and must be skipped by descriptor filtering.
- Reject malformed, truncated, overlapping, oversized, unsupported, and
  decompression-overflow payloads without panicking or partially installing a
  face.

## Implementation

`font.rs` now normalizes WOFF 1.0 payloads into bounded SFNT bytes before the
existing parser path. The decoder sorts table records, supports zlib table
compression through the existing Rust-only `flate2` backend, verifies output
lengths, and recalculates the SFNT checksum adjustment. The same normalizer is
used by parseability checks and document-resource admission. The page-realm
FontFace format filter no longer claims WOFF2, EOT, SVG, or variation formats
until corresponding decoders and renderer capabilities exist.

## Tradeoffs and explicit limits

WOFF decoding adds a temporary reconstructed SFNT allocation and the optional
`flate2` dependency, but reuses the existing rasterizer, shaper, resource
limits, and two-crate boundary. WOFF2's Brotli and transformed-table pipeline,
variable-axis selection, color glyph rasterization, EOT/SVG fonts, and full
font-format/Web IDL parity remain separate issue #40 gates. A valid WOFF whose
decoded SFNT exceeds the native 4 MiB limit is intentionally rejected.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked content_process_font_destination_uses_the_native_font_loader -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked script_font_face_source_list_tries_supported_fallbacks -- --nocapture --test-threads=1`
- `python3 scripts/check-release-documentation.py --require-previous-version`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

