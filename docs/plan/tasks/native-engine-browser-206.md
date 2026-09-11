# Native engine browser slice 206: smooth and arc SVG paths

Status: completed locally.

## Objective

Extend the shared native SVG path surface to cover the reflected Bézier and
elliptical-arc commands used by ordinary icons, diagrams, and chart glyphs.

## Contract

- SVG paths accept absolute and relative `S`, `T`, and `A` commands, including
  repeated parameter tuples after one command letter.
- `S` reflects the preceding cubic control point only after `C`/`S`; `T`
  reflects the preceding quadratic control point only after `Q`/`T`. Other
  preceding commands use the current point as the implicit control point.
- Arc radii are normalized to positive values, radii are expanded when the
  endpoint geometry requires it, and rotation is applied in the SVG user
  coordinate system. Zero-radius arcs become straight segments and coincident
  endpoints produce no arc segment, matching the bounded path model.
- Arc flattening uses at most 16 samples per quarter-turn and 64 samples per
  arc. The existing 2,048-point subpath budget, finite-number checks, and
  invalid `large-arc`/`sweep` flag rejection remain hard limits.
- Flattened points continue through the existing `NativeSvgSubpath`, typed
  `SvgPathFill`/`SvgPathStroke`, layout bounds, software raster, clipping,
  scroll translation, capture, and hit-test owners.
- Unsupported commands, malformed tuples, non-finite values, invalid flags,
  and over-limit geometry fail closed without publishing partial geometry.

## Implementation

The path evaluator now keeps the prior command and the last quadratic/cubic
control point so smooth commands can reflect controls without adding a second
geometry representation. Elliptical arcs use the SVG endpoint-to-center
conversion, normalize radii, choose the requested large-arc/sweep branch, and
append bounded integer-pixel samples to the same subpath consumed by all
existing paint paths.

## Tradeoffs and follow-up

Fixed flattening preserves deterministic CPU, memory, and software-surface
behavior, but it is not an adaptive curve-error guarantee. Integer-pixel
rounding can merge nearby samples, and the bounded raster still does not yet
implement SVG transforms, viewBox/preserveAspectRatio mapping, dash arrays,
explicit line caps/joins, markers, gradients, text layout, filters, or
external SVG resources. Those remain separate issue #40 browser-completeness
work and must reuse the shared path contract when added.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine svg -- --nocapture`
  (7 passed, 0 failed)

Remote CI, push, release, registry publication, and browser-complete
certification remain unclaimed for this local-only checkpoint.

Implementation checkpoint: `807f4122`.
