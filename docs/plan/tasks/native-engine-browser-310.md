# Glass native engine browser slice 310: Canvas 2D surfaces

Status: completed locally; issue #40 production parity and remote CI remain
open.

## Objective

Give page scripts a persistent, bounded Canvas 2D surface that participates in
the native layout, display-list, raster, screenshot, and document-snapshot
paths. A canvas must be useful across script turns rather than being an
unconnected JavaScript-only object.

## Contract

- `HTMLCanvasElement` exposes persistent `width` and `height` dimensions,
  `getContext('2d')`, `toDataURL()`, `toBlob()`, and context attributes.
- The bounded 2D context supports fills, clears, strokes, paths, arcs,
  transforms, save/restore, gradients, alpha, the principal compositing
  modes, image-data reads/writes, canvas-to-canvas `drawImage`, text metrics,
  and bounded block text rendering.
- Drawing state and pixels survive later evaluations in the same document
  realm. Resetting either canvas dimension clears the backing surface and
  resets its drawing state.
- Canvas pixels are transferred to Rust through an explicit per-evaluation
  commit command, validated against the canvas element, dimension, area, and
  RGBA byte limits, and retained in the document snapshot.
- Native layout supplies the canvas intrinsic size, display-list painting emits
  the retained bitmap, and screenshot capture therefore observes actual canvas
  pixels. Detached canvases release their native resources.
- PNG data URLs and blobs are generated from the same bounded RGBA surface;
  malformed or oversized image-data requests fail without unbounded
  allocation.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The JavaScript realm keeps one bounded RGBA surface per canvas node index and
coalesces its latest pixels into `CanvasCommit` at the existing script-turn
boundary. Rust validates and owns the transferred surface, serializes it in
the content and script snapshots, and paints it through the existing image
display command. Canvas resources are removed with detached subtrees. The
JavaScript implementation includes a small persistent 2D state/path model,
source-over/copy/clear/lighter and destination-over blending, gradient
sampling, PNG encoding, and the supported image-data/text operations without
adding another native renderer dependency.

## Tradeoffs and follow-up

The surface is deliberately bounded to 1,048,576 pixels, 4 MiB of RGBA data,
and a 4,096-pixel axis. This protects the script boundary and keeps the
software raster path predictable, but it is not a complete Canvas/Web IDL
implementation: image/bitmap/video sources, compositing and filter breadth,
font loading and shaping, clipping edge cases, pixel-perfect path semantics,
GPU acceleration, accessibility exposure, and complete descriptor/exception
parity remain issue #40 work. The current implementation also keeps canvas
painting in the existing software display list, so replacing CDP still
requires the broader navigation, CSS, networking, workers, input, storage,
security, and conformance gates.

## Verification

- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_canvas_2d_script_and_raster_pipeline --locked -- --nocapture`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo fmt --all -- --check`
- `git diff --check`
- repository documentation coverage, depth, and release-documentation
  validators

The focused witness passes locally. It covers identity and persistence,
dimension/layout behavior, fill/clear, `ImageData`, PNG export, canvas-to-
canvas drawing, stroke/save/restore, gradient paint, detached-node cleanup,
and screenshot pixels. Remote CI, publication, release, and final native/CDP
parity or production-promotion claims remain pending the wider issue #40
gates.
