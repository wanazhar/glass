# Native engine browser slice 203: SVG line and polygon paint

Status: completed locally.

## Objective

Expand the native SVG presentation surface with the basic line geometry used
by icons, diagrams, charts, and simple illustrations: `line`, `polyline`, and
`polygon`.

## Contract

- SVG `line` consumes `x1`, `y1`, `x2`, and `y2`; `polyline` and `polygon`
  consume a bounded comma/whitespace-separated `points` list.
- Parsed points are retained as one shared geometry source for layout bounds,
  display-list commands, software rasterization, and hit-test boxes.
- The display list emits typed `SvgPolyline` stroke commands. `polygon` also
  emits a typed `SvgPolygonFill` command when its fill is not `none`; polygon
  fill uses the deterministic even-odd rule.
- Line paint honors the existing SVG stroke color/width path, including
  inline declarations, `currentColor`, `none`, zero width, clipping, scroll
  translation, and alpha composition.
- Point lists are capped at the native SVG point bound and malformed/odd lists
  are rejected without publishing partial geometry. Line bounds include both
  endpoints so point-hit and paint ownership share the same box.
- The integer software surface uses centerline-to-segment distance for stroke
  coverage and keeps the existing layout/raster ownership boundary.

## Implementation

The layout module now exposes bounded line/point parsing and computes bounds
for all three primitives. The paint builder routes polygon fill before its
stroke and shares the existing SVG color/width normalization. The rasterizer
adds clipped even-odd polygon filling and bounded segment-distance stroke
replay, including optional closing of the final polygon segment.

## Tradeoffs and follow-up

Coordinates remain integer logical pixels and the point list is intentionally
bounded. Segment-distance coverage gives deterministic round endpoint
coverage but does not add SVG path commands, dash arrays, explicit cap/join
styles, transforms, markers, gradients, or viewBox mapping. Those features can
extend the typed SVG command boundary without duplicating layout ownership.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_svg_lines_and_polygons_share_layout_paint_and_hit_test_geometry -- --nocapture`
  (1 passed, 0 failed)

Remote CI, push, release, registry publication, and browser-complete
certification remain unclaimed for this local-only checkpoint.

Implementation checkpoint: `bbf1c522`.
