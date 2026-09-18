# Native-engine browser slice 536: affine COLRv1 sweep gradients

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Remove the conformal-transform boundary for COLRv1 sweep gradients while
preserving bounded native rasterization, color-line extension semantics, and
fail-closed handling for invalid paint transforms.

## Scope

- Retain sweep centers and angle endpoints in gradient space while carrying a
  bounded finite nonsingular affine transform through the native paint path.
- Inverse-map each raster sample before computing the sweep angle, preserving
  skew, non-uniform scale, rotation, reflection, translation, synthetic
  stretch, and glyph-local pixel mapping.
- Keep the existing finite-coordinate, nonzero-span, stop-count, and
  unsupported-paint admission boundaries; no CDP or remote-browser path is
  introduced.

## Contract

- Sweep angle semantics are evaluated in the original gradient coordinate
  system; affine transforms change sample coordinates, not authored angle
  endpoints.
- Transform composition order is explicit: paint-space transforms compose with
  glyph-local pixel mapping, and raster samples use the inverse of that
  composed transform.
- Singular, non-finite, oversized, or otherwise unbounded transforms fail
  closed to the existing monochrome fallback.

## Verification

- The transformed sweep sampling regression passed 1/1.
- The native font unit group passed 41/41.
- The locked `glass-browser` library check passed after removing the obsolete
  conformal-only helper.
- Documentation depth, coverage, and release-truth validators passed.
- The locked `glass-browser` library regression passed 1,252 tests with 1
  ignored.
- The focused COLRv1 native font group passed 7/7, and the native integration
  gate `native_real_font_metrics_feed_layout_and_glyph_paint_when_available`
  passed 1/1.
- Both package checks, the `glass` debug build, locked metadata, and
  `cargo fmt --all -- --check` passed.
- No all-target, cross-platform, release, remote CI, push, or issue-closure
  claim is made by this checkpoint.
