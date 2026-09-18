# Native-engine browser slice 544: FontFace display descriptor validation

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Make dynamic page-realm `FontFace.display` follow the bounded CSS
`font-display` descriptor contract already used by static CSS faces.

## Scope

- Normalize constructor and setter values for `FontFace.display` to lowercase
  `auto`, `block`, `swap`, `fallback`, or `optional`.
- Reject empty, multi-token, and unsupported display values with a bounded
  `SyntaxError` while preserving the previous value on setter failure.
- Keep static CSS-face projection, loading status, source admission, and
  Service Worker interception unchanged.

## Contract

- Missing descriptors default to `auto`; accepted values are trimmed and
  lowercased. Invalid values fail at the page-realm constructor/setter rather
  than waiting for a later font load.
- A rejected setter is transactional: the prior `display` value remains
  observable. Static CSS faces continue to use the native parser's normalized
  descriptor.
- This closes descriptor validation only. Actual block/swap/fallback/optional
  render timing, installed-font discovery, and complete FontFace/Web IDL parity
  remain issue #40 gates.
- No CDP fallback, network behavior, or font bytes change.

## Verification

- Focused `script_font_face_display_descriptor_normalizes_and_rejects_invalid_values`
  filter: 1 passed.
- `cargo check -p glass-browser --lib --locked` passed.
- Serialized native library suite: 1269 passed, 1 ignored.
- `cargo check -p glass-dev --lib --bins --locked` passed.
- `cargo build -p glass-dev --bin glass --locked` passed.
- `cargo metadata --no-deps --format-version 1 --locked` passed.
- `cargo fmt --all -- --check`, documentation depth, coverage,
  release-truth, and `git diff --check` gates passed: 1194 Markdown files,
  93 current guides, 19 substantive contracts, 63 previous-version hits,
  1367 semantic-audit hits, 0 current-claim failures.
- No remote CI, push, release, or issue mutation was performed.
