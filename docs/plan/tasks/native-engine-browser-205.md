# Native engine browser slice 205: SVG curve paths

Status: completed locally.

## Objective

Make common curved SVG icons and diagrams render through the native path
surface by adding bounded quadratic and cubic Bézier segments.

## Contract

- SVG paths additionally accept absolute and relative `C` and `Q` commands,
  including repeated control/end-point tuples after one command letter.
- Curves are evaluated with a fixed bounded segment count and flattened into
  the existing `NativeSvgSubpath` point representation. Relative arithmetic is
  performed before integer logical-pixel rounding.
- The existing typed `SvgPathFill` and `SvgPathStroke` commands consume the
  flattened subpaths, so fill, stroke, clipping, scroll translation, alpha
  composition, capture, and hit-test bounds remain shared.
- Malformed command sequences, unsupported path commands, non-finite numbers,
  and over-limit token/point streams fail closed without publishing partial
  geometry.
- Existing straight commands (`M`, `L`, `H`, `V`, `Z`) and open-path implicit
  fill closure retain their prior behavior.

## Implementation

The path tokenizer now recognizes signed decimal and exponent-form numbers
for quadratic/cubic control points. The evaluator tracks the current point,
resolves relative controls/endpoints, samples each curve into the bounded
subpath, and reuses the existing polygon/segment raster routines without
adding another geometry owner.

## Tradeoffs and follow-up

Fixed-count flattening keeps CPU and memory deterministic on the integer
software surface, but it does not yet provide adaptive error tolerances,
smooth/reflected commands, elliptical arcs, dash arrays, explicit cap/join
styles, transforms, viewBox mapping, markers, gradients, or external SVG
resources. Those can extend the same subpath contract.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_svg_curve_paths_flatten_quadratic_and_cubic_segments -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine svg -- --nocapture`
  (6 passed, 0 failed)

Remote CI, push, release, registry publication, and browser-complete
certification remain unclaimed for this local-only checkpoint.

Implementation checkpoint: `5690b789`.
