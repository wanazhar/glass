# Native-engine browser slice 546: FontFace descriptor validation

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Move dynamic `FontFace` descriptor validation to the page-realm constructor and
setter boundary, matching the native install grammar instead of deferring
malformed descriptors until `load()`.

## Scope

- Validate and normalize `style`, `weight`, `stretch`, `unicodeRange`, and
  `variationSettings` values before storing them on dynamic or static projected
  `FontFace` objects.
- Accept the bounded host grammar: normal/italic style, absolute or ascending
  1-1000 weight ranges, named or 50%-200% stretch ranges, bounded Unicode
  ranges, and quoted variation tags with bounded signed decimals.
- Reject malformed, reversed, out-of-range, over-budget, or unsupported values
  with `SyntaxError`; reject descriptor text over the page-realm bound with
  `RangeError`.
- Keep valid descriptor text flowing unchanged in the native install command
  after normalization; preserve the existing `display` validation contract.

## Contract

- Constructor and setter validation is transactional. A failed setter leaves
  the previous descriptor observable; a failed constructor does not create a
  partially initialized face.
- Normalized descriptors are bounded and parser-compatible with
  `NativeDocument::apply_script_font_face_install`, so admitted bytes cannot
  fail later solely because a descriptor was malformed.
- This closes validation for the descriptors consumed by native font install.
  `variant`, feature-setting, override, and actual FontFace loading/display
  timing remain separate issue #40 parity gates. No CDP fallback or font-byte
  policy changes.

## Verification

- Descriptor normalization/rejection and transactional setter test:
  `script_font_face_descriptor_validation_normalizes_and_rejects_invalid_values`;
  1 passed.
- Existing display validation and install command coverage included in the
  `script_font_face_` subset: 11 passed.
- Serialized native library suite: 1270 passed, 1 ignored.
- Package gates passed: `cargo check -p glass-browser --lib --locked`,
  `cargo check -p glass-dev --lib --bins --locked`,
  `cargo build -p glass-dev --bin glass --locked`, and locked metadata.
- Formatting and documentation gates passed: 1196 Markdown files, 93 current
  guides, 19 substantive contracts, 63 previous-version hits, 1367 semantic
  audit hits, and zero current-claim failures.
- `git diff --check` passed.
- Full-workspace clippy remains blocked by the repository's pre-existing
  warning baseline; no unrelated warning cleanup was included.
- No remote CI, push, release, or issue mutation was performed.
