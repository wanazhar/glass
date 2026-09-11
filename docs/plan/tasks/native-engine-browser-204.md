# Native engine browser slice 204: straight SVG paths

Status: completed locally.

## Objective

Add the straight-segment subset of SVG `path` needed by common icons and
diagrams while preserving one shared geometry model across layout, paint,
rasterization, capture, and hit testing.

## Contract

- The native path parser accepts absolute and relative `M`, `L`, `H`, `V`,
  and `Z` commands, repeated coordinate pairs, comma/whitespace separators,
  signed numeric values, decimal values, and exponent notation.
- Parsed paths retain bounded subpaths and explicit close markers. Coordinates
  are rounded into the integer logical-pixel model after relative arithmetic;
  malformed commands or over-limit token/point streams publish no geometry.
- A path's layout box is derived from all parsed subpath points. The display
  list emits typed `SvgPathFill` and `SvgPathStroke` commands, with even-odd
  fill across subpaths and closed-segment stroke coverage.
- Path paint reuses SVG `fill`, `stroke`, `currentColor`, `stroke-width`,
  `none`, clipping, scroll translation, and alpha composition already used by
  primitive shapes.
- A path with no explicit close still fills by the SVG implicit-close rule;
  stroke only closes subpaths carrying `Z`.

## Implementation

The layout module now tokenizes and evaluates the supported path command
subset into bounded `NativeSvgSubpath` records. The paint builder routes those
records into separate typed fill/stroke commands. The software rasterizer
uses the shared even-odd polygon and point-to-segment routines, with signed
viewport clipping and scroll-safe document coordinates.

## Tradeoffs and follow-up

This slice intentionally stops at straight segments: cubic/quadratic arcs,
elliptical arcs, smooth commands, dash arrays, explicit cap/join styles,
transforms, viewBox mapping, markers, gradients, and external resources are
separate geometry/presentation work. Decimal inputs are rounded because the
current native surface is an integer logical-pixel contract.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_svg_straight_paths_share_relative_geometry_and_fill_stroke_paint -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine svg -- --nocapture`
  (5 passed, 0 failed)

Remote CI, push, release, registry publication, and browser-complete
certification remain unclaimed for this local-only checkpoint.

Implementation checkpoint: `7c113878`.
