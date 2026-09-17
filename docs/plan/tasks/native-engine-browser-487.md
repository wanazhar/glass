# Native engine browser-complete slice 487: honor CSS text direction

- Status: complete
- Scope: `native-engine` / horizontal CSS direction in shaped text
- Issue: #40
- Depends on: [native-engine-browser-486](native-engine-browser-486.md)

## Objective

Use the existing computed `direction:ltr|rtl` value when shaping and painting
explicit supported system-font runs, so right-to-left text receives a real
visual coordinate projection while preserving the established LTR path and
fixed-cell fallback behavior.

## Contract

- `NativeTextMetrics` carries the computed horizontal CSS direction for both
  real-font and fixed-cell paths. Missing or unsupported faces still use the
  deterministic fallback metrics, while alignment consumers retain the same
  direction value.
- HarfRust receives `LeftToRight` or `RightToLeft` after segment-property
  guessing. Shaped cluster indices must be monotone in the selected visual
  direction and remain valid UTF-8 source boundaries.
- RTL glyph positions are mirrored from HarfRust's visual pen into the
  left-origin display-list coordinate system. Glyph offsets, cluster spacing,
  justification spacing, run width, and whitespace decoration ranges use the
  same projection.
- LTR shaped runs and the character-by-character recovery path retain their
  prior placement and metric contracts. No mixed-direction run is claimed as
  Unicode bidi support by this slice.

## Implementation

- Thread `DirectionValue` from computed DOM style into `NativeTextMetrics`.
- Set HarfRust's horizontal direction explicitly and validate cluster order in
  the corresponding forward or reverse order.
- Preserve fixed-point run width and per-glyph visual pen positions, then
  mirror RTL glyph origins and whitespace ranges after justification space is
  included.
- Add focused unit coverage for RTL cluster order, width consistency, and
  bounded visual coordinates; retain existing LTR and display-list regressions.

## Tradeoffs and remaining scope

The direction state is already present in the CSS cascade, so this slice adds
no dependency. It improves horizontal RTL runs without adding a Unicode bidi
algorithm or splitting mixed-direction text into directional runs. Mirroring
the shaped output is deterministic and bounded, but it does not provide
language/script negotiation, `unicode-bidi` isolation/embedding, caret and
selection mapping, or vertical writing modes. The fixed-cell fallback remains
available for faces the shaper or rasterizer rejects.

Mixed bidi segmentation and Unicode reordering, CSS writing modes and text
orientation, missing-glyph family fallback, `@font-face` file/network
resources, variable-font instances, grapheme-safe line breaking, and complete
browser text/Web IDL parity remain issue #40 gates.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --lib --locked font_`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_real_font_metrics_feed_layout_and_glyph_paint_when_available`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_css_direction_reaches_real_font_raster_coordinates`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_display_list_is_revisioned_deterministic_and_visibility_aware`
- documentation truth/depth/shortcut/coverage audits
- `git diff --check`
