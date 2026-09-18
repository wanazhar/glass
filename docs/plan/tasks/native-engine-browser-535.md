# Native-engine browser slice 535: transformed COLRv1 clip masks

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Remove the identity-transform-only COLRv1 clip-box boundary while preserving
bounded software rasterization, current-outline provenance checks, and native
fallback behavior.

## Scope

- Transform each finite nonsingular clip rectangle into a bounded convex
  quadrilateral in the current paint space when the COLR painter admits it.
- Retain nested transformed clip polygons as a bounded mask list and test every
  existing supersampled glyph sample against every polygon.
- Preserve outline-generation clip provenance and fail closed for singular,
  non-finite, oversized, malformed, or out-of-bound clip transforms.
- Keep sweep-gradient skew parity and the remaining browser-profile gates
  explicit; no CDP or remote-browser path is introduced.

## Contract

- Rectangle edges are transformed before painting and are never approximated by
  an axis-aligned bounding box.
- Convex polygon containment includes finite boundary samples; nested masks
  implement intersection by requiring all polygons to contain the sample.
- Clip storage and per-sample work remain bounded by the existing clip-depth
  limit and supersampling budget; no unbounded mask surface is allocated.
- Invalid transforms fail closed to the existing monochrome fallback.

## Verification

- The transformed polygon, nested-mask, singular-transform, and raster coverage
  regression passed 1/1.
- The full native font unit group passed 40/40; the existing COLRv1 fixture
  group passed 4/4.
- The native integration gate
  `native_real_font_metrics_feed_layout_and_glyph_paint_when_available`
  passed 1/1 after the clip-mask propagation.
- The locked `glass-browser` library regression passed 1,251 tests with
  1 ignored.
- `cargo fmt --all -- --check`, locked metadata, both package checks, the
  `glass` debug build, and all three documentation validators passed.
- The native CLI emitted an interactive native-page snapshot for `about:blank`.
- No all-target, sweep-skew, cross-platform, release, remote CI, push, or
  issue-closure claim is made by this checkpoint.
