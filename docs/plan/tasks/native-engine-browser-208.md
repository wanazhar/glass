# Native engine browser slice 208: SVG viewBox mapping

Status: completed locally.

## Objective

Map SVG user-space geometry into its declared viewport so ordinary scalable
icons and diagrams retain their authored proportions when the viewport and
`viewBox` use different dimensions.

## Contract

- A `viewBox` must contain exactly four finite numbers with strictly positive
  width and height. The viewport uses finite positive `width` and `height`
  attributes, with the native 300 by 150 SVG defaults when they are omitted.
  Malformed viewBox or viewport data fails closed without partial geometry.
- The default `preserveAspectRatio` is `xMidYMid meet`. The bounded parser also
  accepts the nine `xMin|xMid|xMax` by `YMin|YMid|YMax` alignments, `meet`,
  `slice`, and `none`; the optional `defer` token is accepted before an
  alignment. `none` preserves independent x/y scales, while `meet` and
  `slice` use the requested uniform scale and alignment offset.
- ViewBox mapping is represented by the same affine `NativeSvgTransform` as
  the preceding transform slice. It composes with SVG root/group/shape
  transforms before one bounded integer-point conversion.
- Transformed rect/circle/ellipse/line/polyline/polygon points and path
  subpaths continue through the existing layout bounds, typed fill/stroke
  display commands, clipping, scroll projection, software rasterization,
  capture, and hit-test owners.
- Identity mappings preserve the existing command forms. Unsupported or
  malformed values fail closed rather than publishing a partial mapping.

## Implementation

The SVG layout owner parses the viewBox and preserve-aspect-ratio tuple into a
finite affine scale/translation matrix. The matrix is folded into the existing
ancestor transform walk, so paint and layout consume exactly the same mapped
points and path subpaths. No second viewport or raster path is introduced.

## Tradeoffs and follow-up

The mapping covers the common scalable-icon contract with deterministic
integer software rendering, but point rounding can merge nearby vertices and
the current logical surface does not yet clip `slice` overflow to the SVG
viewport. CSS sizing/percentages, nested SVG viewport placement, dash arrays,
explicit cap/join styles, gradients, markers, filters, text layout, and
external SVG/image resources remain separate issue #40 browser-completeness
work.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine svg -- --nocapture`
  (10 passed, 0 failed)

Remote CI, push, release, registry publication, and browser-complete
certification remain unclaimed for this local-only checkpoint.

Implementation checkpoint: `4950a7d9`.
