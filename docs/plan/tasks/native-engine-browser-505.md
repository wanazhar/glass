# Native engine browser slice 505: preserve font-stretch descriptors

Status: completed locally on the current source line.

## Objective

Preserve bounded `font-stretch` descriptors for CSS `@font-face` rules and
script-created `FontFace` resources across the native document and
content-process boundaries.

## Contract

- Accept the nine standard named stretch values and percentages from 50% to
  200%.
- Accept one value or an ascending two-value descriptor range, with percentages
  quantized to deterministic percentage tenths without floating point state.
- Reject out-of-bounds, reversed, overlong, and malformed descriptor values
  with a typed diagnostic or native configuration error at the existing owner
  boundary.
- Carry CSS descriptor ranges and dynamic `FontFace.stretch` values through
  native resources, content wire snapshots, and the `document.fonts` projection.
- Preserve serde compatibility for older content snapshots and install
  commands by defaulting an absent stretch field to `normal`.
- Keep byte limits, parser admission, transactional install acknowledgement,
  and no-partial-font-book behavior unchanged.

## Implementation

`NativeFontStretchRange` stores percentage tenths with a `1000` normal value.
The CSS `@font-face` parser records normalized named or percentage ranges, and
the native dynamic `FontFaceInstall` path validates and stores the page state's
`stretch` descriptor before decoding font bytes. Resource and content-wire
structures preserve the range, while the static `document.fonts` descriptor
projection and dynamic command tests verify the public transport.

## Tradeoffs and explicit limits

The bounded integer representation avoids floating-point matching drift and
keeps the wire contract compact, at the cost of quantizing authored percentages
to one decimal place. This slice preserves descriptor identity and admission
policy; the computed CSS `font-stretch` property, face-range matching, and
horizontal glyph scaling are deliberately the next rendering slice rather than
claiming that metadata alone changes layout. Font-variant, font-display timing,
variable/color fonts, WOFF2, cross-realm FontFace projection, and complete
FontFace/Web IDL parity remain issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked font_stretch_parser_normalizes_keywords_percentages_and_ranges -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked script_font_face -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked content_process_font_destination_uses_the_native_font_loader -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked document_fonts_projects_css_faces_and_supports_set_operations -- --nocapture --test-threads=1`
- `python3 scripts/check-release-documentation.py --require-previous-version`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`
