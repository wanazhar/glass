# Native engine browser slice 503: select CSS faces by unicode range

Status: completed locally on the current source line.

## Objective

Implement the bounded `unicode-range` descriptor for CSS `@font-face` rules so
multiple admitted faces with the same family, weight, and style can be chosen
by codepoint instead of the first face winning every character.

## Contract

- Parse comma-separated `U+` codepoint, inclusive range, and trailing-wildcard
  forms case-insensitively.
- Reject empty tokens, malformed hex, internal wildcards, reversed ranges,
  codepoints above `0x10FFFF`, and more than 32 ranges.
- Sort and merge overlapping or adjacent ranges for deterministic matching.
- Treat an omitted descriptor as the full Unicode scalar range.
- Carry ranges through page/document font resources and the content-process
  wire snapshot with bounded untrusted-input validation.
- Preserve multiple best-scoring faces for a named family when ranges are
  present, and require both range membership and a real glyph before selecting
  a face.
- Expose the normalized range string through the CSS `document.fonts` face
  projection; script-created FontFace descriptors remain independent.
- Keep the existing family/weight/style selection and fallback behavior when a
  face has no explicit range.

## Implementation

CSS now stores normalized `NativeUnicodeRange` values on each font-face rule.
Resources copy the ranges into the native font book and the content wire
format. Face selection retains all tied candidates for ranged named families;
glyph lookup then filters each candidate by its range before font coverage.
The page-realm `document.fonts` projection receives the canonical `unicodeRange`
descriptor, while absent ranges serialize as `U+0-10FFFF`.

## Tradeoffs and explicit limits

Ranged families retain several tied font faces, increasing bounded face-book
memory and allowing per-character fallback rather than a single first-face
choice. A string spanning disjoint ranges may use the existing character-wise
fallback path when no one face can shape every character; full shaping across
mixed scripts remains an open text-engine gate. Variable axes, font-stretch,
font-variant, color glyphs, font-display timing, WOFF2, and complete CSS
FontFace/Web IDL parity remain issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked font_face_unicode_range_parser_normalizes_hex_wildcards_and_merges -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked unicode_ranges_select_the_matching_document_font_face -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked font_resources -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked content_process_font_destination_uses_the_native_font_loader -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked css_font_face_refresh_dispatches_loading_error_for_new_rules -- --nocapture --test-threads=1`
- `python3 scripts/check-release-documentation.py --require-previous-version`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

