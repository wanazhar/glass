# Native-engine browser slice 547: FontFace variant descriptors

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Close the next page-realm `FontFace` descriptor gap by validating the bounded
`variant` and `featureSettings` surfaces at construction and setter boundaries,
so dynamic and projected faces do not retain values the native text grammar
cannot interpret.

## Scope

- Validate and normalize the bounded `font-variant` shorthand tokens already
  understood by the native CSS parser: ligature groups, caps, position,
  alternates, East Asian, and numeric groups.
- Validate and normalize bounded `font-feature-settings` entries with printable
  four-character tags, optional `on`/`off` or 0..65535 values, duplicate
  last-wins behavior, and the existing 16-entry limit.
- Reject mixed CSS-wide keywords, duplicate mutually exclusive groups, malformed
  tags, out-of-range values, and over-budget descriptors with `SyntaxError`.
- Preserve transactional setter behavior and FontFaceSet identity; no native
  install wire or font-byte policy changes are introduced by this slice.

## Contract

- `normal` is the only standalone reset for both descriptors. Valid variant
  tokens are lowercased and joined with one space; valid feature entries are
  serialized as canonical quoted tags and numeric values.
- A failed constructor creates no partially initialized face. A failed setter
  leaves the prior descriptor value unchanged, including after the face is
  added to `document.fonts`.
- This closes page-realm validation only. Native font installation does not yet
  apply per-face variant or feature settings; display timing, installed-font
  discovery, and complete FontFace/Web IDL parity remain issue #40 gates. No
  CDP fallback.

## Verification

- Variant/feature normalization, rejection, duplicate last-wins behavior,
  FontFaceSet identity, and transactional setter coverage:
  `script_font_face_variant_and_feature_descriptors_normalize_transactionally`;
  1 passed.
- Focused `script_font_face_` coverage: 12 passed.
- Serialized native library suite: 1271 passed, 1 ignored. The first parallel
  run exposed the existing environment-sensitive local-system-font timing
  flake in two pre-existing tests; an immediate serialized retry passed all
  1271 tests.
- Package gates passed: `cargo check -p glass-browser --lib --locked`,
  `cargo check -p glass-dev --lib --bins --locked`,
  `cargo build -p glass-dev --bin glass --locked`, and locked metadata.
- Formatting and documentation gates passed: 1197 Markdown files, 93 current
  guides, 19 substantive contracts, 63 previous-version hits, 1367 semantic
  audit hits, and zero current-claim failures.
- `git diff --check` passed.
- Full-workspace clippy remains blocked by the repository's pre-existing
  warning baseline; no unrelated warning cleanup was included.
- No remote CI, push, release, or issue mutation was performed.
