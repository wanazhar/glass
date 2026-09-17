# Native engine browser-complete slice 486: shape explicit font runs

- Status: complete
- Scope: `native-engine` / LTR font shaping and cluster-aware rasterization
- Issue: #40
- Depends on: [native-engine-browser-485](native-engine-browser-485.md)

## Objective

Use a real shaping stage for explicit supported system faces so the native
text owner can preserve ligatures, positioned marks, clusters, and fractional
advances before rasterization, while keeping the bounded fontdue path for
faces or directions that cannot be admitted safely.

## Contract

- A cached system face retains immutable font bytes and optional HarfRust
  shaper metadata alongside the fontdue rasterizer.
- HarfRust shapes complete LTR runs with beginning/end-of-text flags and a
  pixel scale with fractional precision. HarfRust cluster byte offsets are
  validated against UTF-8 boundaries and converted to source scalar indices
  before they affect spacing or decoration ranges.
- Cluster spacing preserves the existing letter-spacing and word-spacing
  contract without splitting a shaped cluster. Justification spacing applies
  to whitespace clusters, and the resulting run width is shared by layout,
  paint, hit geometry, decoration, and raster replay.
- HarfRust glyph IDs are bounds-checked against the admitted font before
  fontdue indexed rasterization. Position offsets are applied in the same
  baseline coordinate system as the existing glyph owner.
- If HarfRust cannot parse the face, reports an unsafe cluster sequence, sees
  unsupported directionality, or produces a glyph that the rasterizer cannot
  admit, the existing per-character fontdue renderer supplies the bounded
  recovery result.

## Implementation

- Add optional HarfRust 0.13.3 behind the existing `native-engine` feature,
  with default features disabled and only its `std` feature enabled.
- Retain font bytes for each allowlisted system face and build reusable
  `ShaperData` once during the process font-book load. Enable fontdue
  substitution glyph loading so shaped glyph IDs have indexed outlines.
- Shape LTR Unicode buffers, validate monotone UTF-8 clusters, assign source
  spacing at cluster ends, and convert fixed-point positions into bounded
  display-list coordinates.
- Rasterize shaped glyph IDs with fontdue, preserve whitespace ranges for
  decoration skipping and justification, and retain the old character path
  as a fail-closed recovery route.
- Add unit coverage for cluster order, spacing, shaped width consistency,
  whitespace ranges, and the existing real-font raster contract.

## Tradeoffs and remaining scope

HarfRust adds a pure-Rust OpenType parser/shaper and its transitive table and
buffer dependencies to native-engine builds. The feature remains optional,
and ICU is not enabled, so the build avoids an additional Unicode service
stack. Caching shaper metadata and keeping immutable font bytes increases
first font-book load and resident memory; enabling fontdue substitution
loading increases that cost further so GSUB-produced glyph IDs can be
rasterized safely. The recovery path protects deterministic behavior for
unsupported or malformed inputs, but it does not make those inputs shaped.

This slice is LTR-only. CSS direction and writing-mode propagation, complete
bidi and language/script negotiation, fallback selection for missing glyphs,
`@font-face` file/network resources, arbitrary installed-font discovery,
variable-font instances, color fonts, grapheme-safe line breaking, hinting,
and complete browser text/Web IDL parity remain issue #40 gates.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --lib --locked font_`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_real_font_metrics_feed_layout_and_glyph_paint_when_available`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_display_list_is_revisioned_deterministic_and_visibility_aware`
- documentation truth/depth/shortcut/coverage audits
- `git diff --check`
