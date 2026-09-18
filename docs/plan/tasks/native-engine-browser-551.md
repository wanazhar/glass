# Native-engine browser slice 551: CSS FontFace size-adjust

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Carry CSS `size-adjust` through static `@font-face` rules and dynamic page-realm
`FontFace` descriptors so selected native font faces use the adjusted size for
metrics, shaping, and rasterization.

## Scope

- Parse `size-adjust` as a bounded percentage from 25% through 400%, normalize
  accepted values to the native tenth-percent representation, and format the
  static and dynamic descriptors canonically.
- Preserve the descriptor on `NativeFontFaceRule`, dynamic install commands,
  native font resources, and the content-process wire; missing legacy wire and
  command fields retain the 100% default.
- Include the descriptor in static `FontFace` projection and identity/status
  matching, and reject malformed dynamic values before resource mutation.
- Apply the selected face's adjustment to line metrics, advances, kerning,
  HarfRust shaping scale, and outline, color, SVG, bitmap, and variable-font
  rasterization. System-discovered faces remain at 100%.
- Keep font-display timing, other CSS font metric descriptors,
  installed-font discovery, bitmap variation axes, hinting, and complete
  FontFace/Web IDL parity as separate issue #40 gates.

## Contract

- The accepted CSS percentage range is inclusive: 25% through 400%. Values
  are represented internally in bounded tenth-percent units (`1000` is 100%),
  with canonical output at one decimal place when needed.
- Empty or omitted dynamic descriptors and missing legacy serialized fields use
  100%. Invalid values fail with the existing bounded configuration/descriptor
  errors and do not alter the document's admitted font resources.
- Static `FontFace.sizeAdjust` reflects the normalized stylesheet value; static
  face identity/status matching includes it. Dynamic installs carry and
  revalidate the same value before admission.
- The adjustment scales the selected face's native size inputs; the CSS element
  `font-size` remains unchanged. No CDP fallback or font-byte policy
  relaxation is introduced.

## Implementation

- `css.rs` owns the bounded parser, formatter, default, and static descriptor
  storage.
- `javascript.rs` validates constructor/setter values, includes `sizeAdjust`
  in static identity, and carries it in `FontFaceInstall` commands.
- `dom.rs` validates the dynamic owner boundary, projects static descriptors,
  serializes/deserializes the content-process wire, and applies the resource
  default for legacy payloads.
- `font.rs` carries the value into native faces and applies it consistently to
  metrics, shaping/kerning, and every native raster path.

## Verification

- Focused parser and static projection checks passed:
  `font_face_parser_collects_named_data_sources_without_creating_style_rules`,
  `font_size_adjust_parser_enforces_percentage_bounds`, and
  `css_font_face_refresh_dispatches_loading_error_for_new_rules`.
- Dynamic and owner-boundary checks passed:
  `script_font_face_descriptor_validation_normalizes_and_rejects_invalid_values`,
  `script_font_face_load_installs_bounded_inline_bytes`,
  `content_wire_round_trips_document_font_resources`, and
  `script_font_face_install_rejects_malformed_size_adjust`.
- Native scaling coverage passed:
  `font_face_size_adjust_scales_metrics_and_rasterization`.
- Full locked native library suite passed serially: 1278 passed, 1 ignored,
  1277 filtered; serial execution preserves the existing shared system-font
  fixture invariant.
- Package gates passed: `cargo check -p glass-browser --lib --locked`,
  `cargo check -p glass-dev --lib --bins --locked`, `cargo build
  -p glass-dev --bin glass --locked`, and locked metadata.
- Documentation depth passed: 93 current guides and 19 substantive contracts.
  Coverage passed for 1201 Markdown files, 346 full-product MCP tools (101
  browser-only), 17 examples, and 22 public modules. Release truth passed for
  the same 1201 Markdown documents with current=83, previous-version hits=63,
  semantic hits=1367, and zero current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- The local conventional commit is recorded after final tree inspection.
- No CDP fallback, remote issue mutation, push, or release action was used.
