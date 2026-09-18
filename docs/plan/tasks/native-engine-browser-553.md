# Native-engine browser slice 553: FontFace display carry

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Carry the normalized CSS `font-display` value through static stylesheet
projection and dynamic page-realm `FontFace` installation without claiming the
separate font-display timing algorithm is complete.

## Scope

- Preserve `auto`, `block`, `swap`, `fallback`, and `optional` on dynamic
  `FontFaceInstall` commands, native font resources, and the content-process
  wire; missing legacy command and wire fields use `auto`.
- Include `font-display` in static `FontFace` identity/status matching so a
  descriptor change cannot reuse an identity that describes a different
  display policy.
- Revalidate the dynamic value at the document owner boundary before resource
  mutation and reject unknown values transactionally.
- Keep font-display timing, block/swap/fallback periods, installed-font
  discovery, variable-axis completeness, hinting, media output, and complete
  FontFace/Web IDL parity as separate issue #40 gates.

## Contract

- Internal and wire representation uses the existing `NativeFontDisplay`
  enum. Empty or omitted dynamic values and missing legacy serialized fields
  map to `Auto`; canonical script and stylesheet projection uses the enum's
  lowercase keyword.
- Dynamic owner validation accepts only `auto`, `block`, `swap`, `fallback`, or
  `optional`, and invalid values fail with the existing bounded configuration
  error before the document's admitted resources change.
- Static identity/status matching compares `font_display` in addition to the
  existing family, weight, variation, feature, size-adjust, and metric fields.
- This slice carries policy state only. It does not implement timed fallback,
  swap, or optional rendering behavior and introduces no CDP fallback or font
  byte-policy relaxation.

## Implementation

- `javascript.rs` carries `FontFace.display` in install commands and includes
  it in the static descriptor identity key.
- `dom.rs` validates dynamic display values, projects static status with the
  display field, and serializes/deserializes it with legacy `auto` defaults.
- `font.rs` preserves display metadata on resources and native selected faces;
  system-discovered faces remain `auto`.

## Verification

- Focused dynamic projection and validation passed:
  `script_font_face_descriptor_validation_normalizes_and_rejects_invalid_values`,
  `script_font_face_load_installs_bounded_inline_bytes`, and
  `script_font_face_install_rejects_malformed_display`.
- Static identity/status projection passed:
  `css_font_face_refresh_dispatches_loading_error_for_new_rules`.
- Content wire compatibility passed:
  `content_wire_round_trips_document_font_resources`, including legacy missing
  field defaults.
- The locked native library suite passed with
  `RUST_MIN_STACK=8388608`: 1282 passed, 1 ignored, and 1281 filtered across
  the two emitted test suites.
- Package gates passed: `cargo check -p glass-browser --lib --locked`,
  `cargo check -p glass-dev --lib --bins --locked`, `cargo build
  -p glass-dev --bin glass --locked`, and locked metadata.
- Documentation depth passed: 93 current guides routed/audited and 19
  substantive contracts. Coverage passed for 1203 Markdown files, 346
  full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules. Release truth passed for the same 1203 Markdown documents with
  current=83, previous-version hits=63, semantic hits=1367, and zero
  current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed after final edits.
- No CDP fallback, remote issue mutation, push, or release action was used.
