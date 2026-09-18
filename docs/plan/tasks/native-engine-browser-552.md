# Native-engine browser slice 552: CSS FontFace metric overrides

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Carry CSS `ascent-override`, `descent-override`, and `line-gap-override` through
static `@font-face` rules and dynamic page-realm `FontFace` descriptors so
selected native faces use authored line metrics after `size-adjust`.

## Scope

- Parse the three metric descriptors as `normal` or bounded percentages from
  0% through 1000%, normalize accepted values to bounded tenth-percent units,
  and format static and dynamic descriptors canonically.
- Preserve the metric override struct on `NativeFontFaceRule`, dynamic install
  commands, native font resources, and the content-process wire; missing legacy
  wire and command fields retain the `normal` defaults.
- Include all three descriptors in static `FontFace` projection and
  identity/status matching, and reject malformed dynamic values before
  resource mutation.
- Apply overrides to selected-face ascent, descent, and line-gap calculations
  after the face's `size-adjust` scale; system-discovered faces retain native
  metrics.
- Keep font-display timing, installed-font discovery, variable-axis
  completeness, hinting, media output, and complete FontFace/Web IDL parity as
  separate issue #40 gates.

## Contract

- `normal` is represented internally as `None`; percentages are represented in
  bounded tenth-percent units (`1000` is 100%). The inclusive accepted range is
  0% through 1000%, with canonical output at one decimal place when needed.
- Empty or omitted dynamic descriptors and missing legacy serialized fields use
  `normal`. Invalid values fail with the existing bounded configuration or
  descriptor errors and do not alter the document's admitted font resources.
- Static `FontFace.ascentOverride`, `FontFace.descentOverride`, and
  `FontFace.lineGapOverride` reflect normalized stylesheet values; static face
  identity/status matching includes all three. Dynamic installs carry and
  revalidate the same values before admission.
- Override percentages apply to the selected face size after `size-adjust`;
  element `font-size` remains unchanged. No CDP fallback or font-byte policy
  relaxation is introduced.

## Implementation

- `css.rs` owns the metric override type, bounded parser, formatter, and static
  descriptor storage.
- `javascript.rs` validates constructor/setter values, includes all three
  descriptors in static identity, and carries them in `FontFaceInstall`
  commands.
- `dom.rs` validates the dynamic owner boundary, projects static descriptors,
  serializes/deserializes the content-process wire, and applies legacy defaults.
- `font.rs` carries overrides into native faces and replaces selected-face line
  metrics while preserving the existing native metrics when descriptors are
  `normal`.

## Verification

- Static parser and projection checks passed:
  `font_face_parser_collects_named_data_sources_without_creating_style_rules`,
  `font_metric_override_parser_accepts_normal_and_bounded_percentages`, and
  `css_font_face_refresh_dispatches_loading_error_for_new_rules`.
- Dynamic and owner-boundary checks passed:
  `script_font_face_descriptor_validation_normalizes_and_rejects_invalid_values`,
  `script_font_face_load_installs_bounded_inline_bytes`,
  `content_wire_round_trips_document_font_resources`, and
  `script_font_face_install_rejects_malformed_metric_override`.
- Native line-metric coverage passed:
  `font_face_metric_overrides_replace_native_line_metrics`.
- The locked native library suite passed with
  `RUST_MIN_STACK=8388608`: 1281 passed, 1 ignored, and 1280 filtered across
  the two emitted test suites. The larger test stack is required by the
  existing Clap parser test on this runner; the metric-focused tests also pass
  at the default stack.
- Package gates passed: `cargo check -p glass-browser --lib --locked`,
  `cargo check -p glass-dev --lib --bins --locked`, `cargo build
  -p glass-dev --bin glass --locked`, and locked metadata.
- Documentation depth passed: 93 current guides routed/audited and 19
  substantive contracts. Coverage passed for 1202 Markdown files, 346
  full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules. Release truth passed for the same 1202 Markdown documents with
  current=83, previous-version hits=63, semantic hits=1367, and zero
  current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed after final edits.
- The local conventional commit is recorded after final tree inspection.
- No CDP fallback, remote issue mutation, push, or release action was used.
