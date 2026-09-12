# Native engine browser slice 228: SVG image resources

Status: completed locally.

## Objective

Connect the existing native SVG document/layout/raster pipeline to the shared
image resource owner so SVG images behave as real image resources in local
data URLs, CSS backgrounds, HTTP(S) content-process pages, and responsive
`picture` source selection.

## Contract

- `data:image/svg+xml` accepts bounded base64 and percent-encoded SVG payloads;
  validated HTTP(S) `image/svg+xml` responses use the same decoder registry.
- The SVG image decoder parses one standalone SVG document through the native
  DOM, reuses SVG layout, viewBox mapping, display-list, clipping, and software
  raster owners, and publishes one RGBA `NativeImage` with intrinsic dimensions.
- Numeric and `px` SVG viewport dimensions are accepted. Missing dimensions use
  the native SVG default viewport, while a single missing dimension can derive
  from a valid viewBox ratio. Invalid, non-finite, percentage, zero, or
  over-budget dimensions fail closed.
- `<img>`, one CSS `background-image` URL, and `<picture><source>` selection
  share the existing intrinsic-state, current-source, cache, typed-wire,
  content-process, paint, capture, and load/error lifecycle owners.
- SVG image decoding is bounded by document size, raster surface pixels,
  decoded RGBA bytes, and the existing content-process transfer budget. A
  nested data SVG image or data-SVG background is rejected before recursive
  decode; ordinary raster descendants do not create an unbounded recursion
  path.
- SVG image parsing does not execute page scripts or fetch standalone external
  subresources. The outer document remains the owner of network policy and
  resource loading.

## Implementation

`image.rs` now recognizes `image/svg+xml` and renders the first SVG root with
the existing `NativeDocument` and `NativeSurface` APIs. A thread-local decode
depth guard plus document walk rejects recursive data-SVG resources before
paint. The image decoder derives bounded intrinsic dimensions, including
common `px` values and one-sided viewBox ratios, then transfers the resulting
pixels through the pre-existing image wire without adding a crate.

`resource_loader.rs` admits the HTTP content type and `dom.rs` admits the
`picture` source type. `layout.rs` uses the same `px` length interpretation
for inline SVG intrinsic sizing and viewBox transforms. Integration witnesses
cover data-image paint plus CSS background paint, and an HTTP content-process
`picture` source with intrinsic dimensions and display-list pixels.

## Tradeoffs and follow-up

Rendering SVG through the established native display-list keeps shape geometry,
viewBox transforms, clipping, and raster behavior consistent with inline SVG
and avoids a second native image library or a larger build graph. It spends
bounded CPU and memory per decode and produces full RGBA pixels, which makes
cache, IPC validation, and replay deterministic at the cost of retaining a
decoded surface rather than the source document.

Nested SVG image graphs, external resources inside an image document, image
filters/masks/gradients, and full SVG image-document navigation remain
follow-up browser-completeness work. They must extend this owner with explicit
resource and recursion budgets rather than bypassing the native process
boundary.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_svg_ -- --nocapture` (10 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_svg_picture_source -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine image -- --nocapture` (16 passed, 0 failed)

Implementation checkpoint: `81adbf61`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
