# Native-engine browser slice 534: affine radial gradient geometry

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Close the COLRv1 radial-gradient transform gap left by slice 533 without
weakening the bounded native renderer. Preserve exact two-circle conical
sampling while allowing finite nonsingular affine paint transforms to produce
elliptical radial geometry.

## Scope

- Keep the gradient's two circles in gradient space instead of collapsing them
  to transformed center/radius approximations.
- Carry a bounded affine transform through glyph-local pixel mapping and
  synthetic horizontal stretch.
- Inverse-map each finite raster sample before applying the exact radial
  equation; reject singular, non-finite, or out-of-bound transforms.
- Preserve existing conformal-only sweep behavior and identity-only clip-box
  behavior; those remain separate issue #40 gates.

## Contract

- Translation, rotation, uniform and non-uniform scale, reflection, and skew
  are accepted for radial paints when the affine transform is finite,
  nonsingular, and within the native transform bound.
- Gradient sampling remains the exact highest non-negative-radius root from
  slice 533, with pad/repeat/reflect color-line extension and transparent
  no-intersection samples.
- Pixel mapping and synthetic stretch compose with the radial transform rather
  than rewriting the circle geometry.
- Singular or invalid transforms fail closed to the existing monochrome
  fallback; no CDP or remote-browser path is introduced.

## Verification

- The affine radial sampler and pixel-mapping regressions passed 3/3:
  exact two-circle geometry, transparent no-intersection compositing, and
  transformed ellipse sampling.
- The full native font unit group passed 39/39, including singular-transform
  rejection and affine transform admission; the existing COLRv1 fixture group
  passed 4/4.
- The real-font native integration gate
  `native_real_font_metrics_feed_layout_and_glyph_paint_when_available`
  passed 1/1.
- The locked `glass-browser` library regression passed 1,250 tests with
  1 ignored on the successful rerun; an earlier parallel run exposed two
  concurrent `FontFace local()` failures, so the follow-up focused 9/9
  `script_font_face` run and full rerun are retained as the reproducible
  evidence for this checkpoint.
- `cargo fmt --all -- --check`, locked metadata, both package checks, the
  `glass` debug build, and all three documentation validators passed.
- No all-target, transformed-clip, sweep-skew, cross-platform, release,
  remote CI, push, or issue-closure claim is made by this checkpoint.
