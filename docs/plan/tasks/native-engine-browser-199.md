# Native engine browser slice 199: bounded inline SVG shape rendering

Status: completed locally.

## Objective

Give the native renderer a useful inline SVG surface for ordinary page icons,
charts, and controls. SVG shapes must participate in the same layout, paint,
capture, and hit-test pipeline as HTML elements.

## Contract

- Inline `svg` elements use explicit integer `width`/`height` attributes as
  intrinsic dimensions, with the bounded SVG defaults when omitted.
- `rect`, `circle`, and `ellipse` descendants are placed in SVG coordinates;
  bounded `g` descendants recurse without changing their coordinate system.
- SVG shape boxes are visible layout artifacts and retain their element identity
  for geometry and hit testing.
- Shape fills accept the existing native color grammar through a `fill`
  presentation attribute or inline `style`, including `none`.
- Circle and ellipse fills are converted to bounded deterministic scanline
  commands in the existing software display-list/raster pipeline.
- The existing CSS diagnostics surface recognizes `fill` and `stroke` paint
  values so valid SVG presentation styling is not reported as an unsupported
  declaration.

## Implementation

The layout builder now gives SVG replaced content intrinsic dimensions and
recursively places supported shape descendants. The paint builder emits the
existing bounded rectangle commands for rectangular fills and deterministic
scanline rectangles for circular/elliptical fills, preserving clipping,
scrolling, opacity, PNG capture, and command limits.

## Tradeoffs and follow-up

This is the first rendering slice for SVG. It intentionally leaves paths,
polygons, polylines, lines, gradients, masks, filters, transforms, viewBox
mapping, stroke geometry, text, external SVG resources, and complete SVG DOM
namespaces for later slices. Unsupported SVG content remains recoverable and
does not enter the native layout claim silently.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_inline_svg_shapes_share_layout_paint_and_hit_test_geometry -- --nocapture`
  (1 passed, 0 failed)
- the witness covers intrinsic dimensions, nested shape placement, attribute
  and inline-style fills, diagnostics, screenshot pixels, and hit testing

Implementation checkpoint: `f3a0ccea`.

Remote CI, push, release, registry publication, and browser-parity
certification remain pending. Native mode remains non-promoted until the
issue-level production gates pass.
