# Native-engine browser slice 550: CSS FontFace feature defaults

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Carry the CSS `@font-face` `font-feature-settings` descriptor through native
parsing, font-resource construction, and the static page-realm `FontFace`
projection so CSS-declared OpenType defaults affect native shaping.

## Scope

- Parse bounded `font-feature-settings` descriptors with the existing
  printable-tag, numeric-value, duplicate-tag, and entry-count rules.
- Preserve normalized feature settings on `NativeFontFaceRule` and
  `NativeFontFaceResource`, including content-process resource construction.
- Project the normalized descriptor through static `document.fonts` faces and
  include it in static-face identity/status matching.
- Keep face defaults below element-authored `font-feature-settings`; matching
  element entries override face values and unmatched face entries suppress
  automatic shaping defaults.
- Keep font-display timing, installed-font discovery, remaining CSS FontFace
  descriptors, and complete FontFace/Web IDL parity as separate issue #40
  gates.

## Contract

- Missing or `normal` `font-feature-settings` yields an empty face-default
  list. Valid entries are serialized canonically as quoted four-byte tags and
  bounded numeric values; duplicate tags retain the last value.
- Invalid CSS descriptor values emit the existing bounded stylesheet
  diagnostic and do not replace the rule's prior/default metadata.
- Static `FontFace.featureSettings` reflects the normalized CSS descriptor;
  resource admission and status identity include the same feature list.
- No CDP fallback, font-byte policy relaxation, or silent fallback is added.

## Verification

- CSS parsing and canonical projection are covered by
  `font_face_parser_collects_named_data_sources_without_creating_style_rules`
  and `css_font_face_refresh_dispatches_loading_error_for_new_rules`.
- Resource carry and shaping precedence are covered by
  `font_face_feature_defaults_merge_with_element_overrides`.
- Focused descriptor checks: CSS parser 1 passed; static projection 1 passed;
  resource/shaping precedence 1 passed.
- Full locked native library suite: 1275 passed, 1 ignored, run serially to
  avoid the existing shared system-font fixture race seen only under parallel
  execution.
- Package gates passed: `cargo check -p glass-browser --lib --locked`,
  `cargo check -p glass-dev --lib --bins --locked`, `cargo build
  -p glass-dev --bin glass --locked`, and locked metadata.
- Documentation depth passed: 93 current guides and 19 substantive contracts.
  Coverage passed for 1200 Markdown files, 346 full-product MCP tools (101
  browser-only), 17 examples, and 22 public modules. Release truth passed for
  the same 1200 Markdown documents with current=83, previous-version hits=63,
  semantic hits=1367, and zero current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- The local conventional commit is recorded after final tree inspection.
- No remote CI, push, release, or issue mutation is performed.
