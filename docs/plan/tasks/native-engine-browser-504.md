# Native engine browser slice 504: carry dynamic FontFace unicode ranges

Status: completed locally on the current source line.

## Objective

Make the page-realm `FontFace` constructor's `unicodeRange` descriptor reach the
native font owner so script-created faces obey the same bounded codepoint
selection contract as CSS `@font-face` resources.

## Contract

- Include the page `FontFace` state's `unicodeRange` in each bounded
  `FontFaceInstall` command.
- Preserve compatibility with older command payloads by treating an omitted
  field as the unrestricted range.
- Parse dynamic descriptors with the same case-insensitive codepoint, range,
  and trailing-wildcard grammar, normalization, merge, and 32-range cap used
  by CSS rules.
- Reject malformed, reversed, out-of-range, or overlong descriptors before
  decoding or admitting font bytes.
- Carry accepted ranges into the document font book and content-process wire
  snapshot without weakening transactional admission or byte limits.
- Keep dynamic `FontFace` failure acknowledgement and `FontFaceSet` settlement
  behavior unchanged for rejected descriptors.

## Implementation

`FontFaceInstall` now transports a serde-defaulted `unicode_range` string from
the page bootstrap. The native document owner parses non-empty values with the
shared CSS unicode-range parser and stores the normalized ranges on the
admitted `NativeFontFaceResource`; an empty/default value remains unrestricted.
Invalid dynamic descriptors fail at the native configuration boundary before
font bytes can be installed. The inline FontFace and content-wire tests cover
the successful range, and a native-owner test covers malformed input rejection.

## Tradeoffs and explicit limits

Dynamic descriptors reuse the CSS parser and resource representation, avoiding
a second range grammar but coupling both surfaces to the same 32-range and
codepoint limits. Rejecting malformed descriptors at the owner boundary keeps
the page acknowledgement transactional, while the JavaScript constructor can
remain pending until its existing host admission path reports the result.
Mixed-script shaping, font-stretch/variant, font-display timing, variable/color
fonts, WOFF2, cross-realm FontFace projection, and complete FontFace/Web IDL
parity remain issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked script_font_face_load_installs_bounded_inline_bytes -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked script_font_face_install_rejects_malformed_unicode_range -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked content_process_font_destination_uses_the_native_font_loader -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked document_fonts_projects_css_faces_and_supports_set_operations -- --nocapture --test-threads=1`
- `python3 scripts/check-release-documentation.py --require-previous-version`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`
