# Native engine browser slice 202: bounded SVG strokes

Status: completed locally.

## Objective

Extend the native SVG presentation path beyond fills so common stroked
`rect`, `circle`, and `ellipse` artwork is visible through the same layout,
display-list, raster, capture, and hit-test surfaces.

## Contract

- Existing SVG shape layout boxes remain the single geometry source for both
  fill and stroke commands.
- The display list emits a typed `SvgStroke` command for `rect`, `circle`, and
  `ellipse`; circles use the normalized ellipse shape family.
- `stroke` accepts the existing native color syntax, `none`, inline-style
  declarations, and `currentColor` through the element's computed text color.
- `stroke-width` accepts bounded non-negative numeric or `px` values. Missing
  or invalid values use the one-pixel initial width, zero suppresses paint, and
  values above the fixed raster bound are clamped.
- Stroke rasterization is deterministic and clipped to the shape's existing
  layout box, with the current viewport scroll and opacity-group paths applied
  by the shared rasterizer. Fill-less stroked shapes retain transparent
  interiors.
- Unsupported SVG paths, lines, joins, caps, transforms, gradients, markers,
  and resource paint remain outside this slice and are not misrepresented as
  supported geometry.

## Implementation

`NativeDisplayCommand` now carries `SvgStroke` plus a public shape-family
enum. The paint builder preserves the existing scanline ellipse fills,
appends stroke commands after fill, and keeps stroke-only shapes paintable.
The software surface replays rectangular strokes with an inside ring and
ellipse strokes with normalized outer/inner ellipse tests. Signed clipping
keeps partially scrolled or viewport-edge shapes safe; alpha blending is
unchanged.

## Tradeoffs and follow-up

The integer logical-pixel surface uses inside-bounded strokes so a stroke
cannot expand layout or overflow an existing clip. Fractional SVG geometry is
rounded to the bounded raster model, and the slice does not introduce a
general SVG path tessellator. The next SVG work can build on this command
boundary for paths, line caps/joins, transforms, and viewBox-aware geometry
without changing the layout/raster ownership split.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_svg_strokes_replay_bounded_rect_and_ellipse_geometry -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_inline_svg_shapes_share_layout_paint_and_hit_test_geometry -- --nocapture`
  (1 passed, 0 failed)

Remote CI, push, release, registry publication, and browser-complete
certification remain unclaimed for this local-only checkpoint.

Implementation checkpoint: `633790da`.
