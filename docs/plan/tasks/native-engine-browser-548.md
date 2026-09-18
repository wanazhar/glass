# Native-engine browser slice 548: FontFace feature application

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Carry page-realm `FontFace.featureSettings` through native installation and the
content-process wire, retain it as a per-face OpenType default, and apply it in
native text shaping without losing the existing element-level precedence.

## Scope

- Serialize normalized dynamic `featureSettings` in the page `FontFaceInstall`
  command, with bounded backward-compatible defaults for older commands.
- Validate feature settings again in the document owner before admitting font
  bytes; reject malformed tags, values, and over-budget lists without changing
  document font resources.
- Preserve feature settings in `NativeFontFaceResourceWire` and reconstruct
  them in content-backed documents with bounded wire validation and legacy
  missing-field defaults.
- Store face defaults on `NativeFontFace`; merge them with element-authored
  `font-feature-settings` so matching element tags override face defaults while
  unmatched face defaults suppress automatic feature defaults.
- Keep CSS `@font-face` resources at the existing empty feature-default state
  until CSS font-face feature descriptors are parsed and admitted separately.
- No CDP fallback, unbounded feature grammar, font-byte policy relaxation,
  variant application, font-display timing, or installed-font discovery changes.

## Contract

- `normal` and an omitted feature descriptor produce an empty bounded feature
  list. Canonical dynamic values carry printable four-byte tags and numeric
  values; duplicate tags are already normalized last-wins in the page realm and
  are re-parsed by the document owner.
- Face defaults are the lower-precedence source. Element-authored entries
  replace matching tags and append new tags within the native 16-entry limit.
  Automatic ligature, kerning, and variant feature insertion observes this
  merged list.
- Content-process wire decoding accepts older snapshots without the optional
  feature field as an empty list, rejects invalid feature metadata, and keeps
  the existing family/weight/stretch/unicode/variation and byte bounds.
- Invalid dynamic feature settings fail before font admission; existing
  `FontFace` state and document resources remain unchanged.

## Verification

- Dynamic page-realm install serialization and content admission cover
  normalized feature settings and wire projection in
  `script_font_face_load_installs_bounded_inline_bytes`; malformed host-side
  feature settings are rejected by
  `script_font_face_install_rejects_malformed_feature_settings`.
- Native face-default and element-override precedence is covered by
  `font_face_feature_defaults_merge_with_element_overrides`.
- Content wire feature round-trip and missing-field compatibility are covered
  by `content_wire_round_trips_document_font_resources`.
- Focused dynamic FontFace coverage: 13 passed.
- Focused native feature-precedence coverage: 1 passed.
- Serialized native library suite: 1273 passed, 1 ignored.
- Package gates passed: `cargo check -p glass-browser --lib --locked`,
  `cargo check -p glass-dev --lib --bins --locked`,
  `cargo build -p glass-dev --bin glass --locked`, and locked metadata.
- Documentation gates passed: depth 93 current guides and 19 substantive
  contracts; coverage 1198 Markdown files, 346 full-product MCP tools
  (101 browser-only), 17 examples, and 22 public modules; release truth 1198
  Markdown files, current=83, previous-version hits=63, semantic hits=1367,
  and zero current-claim failures.
- No remote CI, push, release, or issue mutation was performed.
