# Native-engine browser slice 549: FontFace variant application

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Apply the page-realm `FontFace.variant` descriptor to dynamic native faces by
carrying its normalized value through the install command, validating it again
at the document owner, and translating its bounded CSS font-variant tokens to
OpenType feature defaults.

## Scope

- Serialize normalized dynamic and static-projected `variant` values in the
  page `FontFaceInstall` command, with an omitted-field default for older
  commands.
- Revalidate the bounded variant grammar before admitting font bytes; reject
  malformed or mutually exclusive token groups without mutating document font
  resources.
- Map supported ligature, capitalization, position, alternate, East Asian,
  numeric, ordinal, and slashed-zero tokens to their OpenType feature tags.
- Combine variant-derived defaults with `featureSettings`, giving explicit
  feature entries precedence; preserve the existing element-level
  `font-feature-settings` precedence during shaping.
- Keep CSS `@font-face` descriptor parsing, font-display timing, installed-font
  discovery, and complete FontFace/Web IDL parity as separate issue #40 gates.

## Contract

- `normal` produces no face-level feature defaults. Other values use the same
  normalized, mutually exclusive bounded token grammar as the page realm.
- Variant-derived tags are lower-precedence defaults. A matching
  `FontFace.featureSettings` entry replaces the variant value; an element
  `font-feature-settings` entry then replaces matching face defaults. Unmatched
  face defaults continue to suppress automatic shaping defaults.
- Older serialized install commands without `variant` decode as an empty
  descriptor and retain existing behavior. Invalid host descriptors fail
  before resource admission.
- No CDP fallback or font-byte policy relaxation is introduced.

## Verification

- CSS variant-to-OpenType mapping and rejection are covered by
  `font_face_variant_descriptor_maps_to_bounded_features`.
- Dynamic page-realm install, command normalization, document admission, wire
  projection, and shaping defaults are covered by
  `script_font_face_load_installs_bounded_inline_bytes` and
  `font_face_feature_defaults_merge_with_element_overrides`.
- Host-side malformed variant rejection is covered by
  `script_font_face_install_rejects_malformed_variant`; the existing malformed
  descriptor suite remains green.
- Focused checks: CSS mapping/precedence 1 passed; dynamic FontFace coverage
  14 passed serially; native feature-precedence coverage 1 passed; malformed
  descriptor coverage 5 passed.
- Full locked native library suite: 1275 passed, 1 ignored, run serially to
  avoid the existing shared system-font fixture race seen only under parallel
  execution.
- Package gates passed: `cargo check -p glass-browser --lib --locked`,
  `cargo check -p glass-dev --lib --bins --locked`, `cargo build
  -p glass-dev --bin glass --locked`, and locked metadata.
- Documentation gates passed: depth 93 current guides and 19 substantive
  contracts; coverage and release truth each validated 1199 Markdown files,
  346 full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules; release truth current=83, previous-version hits=63, semantic
  hits=1367, and zero current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- The local conventional commit is recorded after final tree inspection. No
  remote CI, push, release, or issue mutation is performed.
