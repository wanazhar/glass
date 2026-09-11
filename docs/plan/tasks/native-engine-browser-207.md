# Native engine browser slice 207: SVG transform matrices

Status: completed locally.

## Objective

Make common transformed SVG icons and diagrams use the same native geometry
for layout, paint, capture, and hit testing instead of treating the
`transform` attribute as an unrendered decoration.

## Contract

- SVG `transform` attributes are accepted on an SVG root, nested `<g>`, and
  supported SVG shapes. Transform lists accept `matrix`, `translate`, `scale`,
  `rotate`, `skewX`, and `skewY` with the SVG one-, two-, or six-argument
  forms where applicable, using comma or whitespace separators.
- Ancestor transforms compose outside descendant transforms. Transform-list
  operations retain their authored order; `rotate(angle cx cy)` rotates around
  the supplied center.
- Supported `rect`, `circle`, `ellipse`, `line`, `polyline`, `polygon`, and
  `path` geometry is transformed before one bounded integer-point conversion.
  Existing path flattening and the 32-point ellipse sample cap remain shared
  geometry limits.
- Transformed rectangles and ellipses use the existing polygon fill and
  polyline stroke commands. Transformed paths continue to use the typed path
  fill/stroke commands. The transformed bounds are the same bounds consumed by
  layout, clipping, scroll projection, software rasterization, capture, and
  hit testing.
- Unknown transform functions, malformed argument lists, non-finite values,
  unsupported geometry, and over-limit point output fail closed without
  publishing partial display geometry. Identity transforms preserve the
  existing command forms and behavior.

## Implementation

`NativeSvgTransform` owns the bounded affine matrix and finite point mapping.
The layout owner walks the SVG ancestor chain, composes root-to-shape
matrices, and exposes shared transformed point/subpath helpers. Paint asks the
same owner for the matrix and sends transformed shape points through the
existing polygon/polyline/path display-list and software-raster owners.

## Tradeoffs and follow-up

Affine transforms now cover the common SVG icon/diagram transform surface with
deterministic integer software rendering, but point rounding can merge nearby
vertices and ellipse sampling is fixed-count. This slice covers the SVG
`transform` attribute; CSS `transform`, `transform-origin`, SVG
`viewBox`/`preserveAspectRatio`, dash arrays, explicit cap/join styles,
gradients, markers, text layout, filters, and external SVG/image resources
remain separate issue #40 browser-completeness work.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine svg -- --nocapture`
  (9 passed, 0 failed)

Remote CI, push, release, registry publication, and browser-complete
certification remain unclaimed for this local-only checkpoint.

Implementation checkpoint: `abc56314`.
