# Glass native engine browser slice 311: image sources for Canvas 2D

Status: completed locally; issue #40 production parity and remote CI remain
open.

## Objective

Connect the native image pipeline to the page Canvas 2D API. A loaded image
element must be a real drawable source, and the canvas readback contract must
preserve the web security boundary when that source is cross-origin.

## Contract

- Page script snapshots expose bounded decoded pixels for loaded `<img>`
  elements and supported data-image URLs, using the existing image loader and
  image cache rather than a second network path.
- `CanvasRenderingContext2D.drawImage()` accepts a ready canvas or image source
  with the two-, four-, and eight-argument forms, applies the destination
  transform, clips source samples to the source surface, and preserves the
  source alpha/compositing behavior.
- Drawing a tainted cross-origin image marks the destination canvas
  origin-unclean and propagates that state through later canvas-to-canvas
  draws. The image can render, but `getImageData()`, `toDataURL()`, and
  `toBlob()` raise `SecurityError` on the tainted surface.
- Same-origin, data, and local fixture image sources remain origin-clean;
  canvas reset creates a fresh origin-clean surface. The origin-clean bit is
  transferred with every `CanvasCommit` and retained in document snapshots.
- Image-resource and pixel transfers remain bounded by the existing native
  image limits and the Canvas limits; malformed entries are ignored in the
  page projection and rejected at the Rust boundary.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

`NativeDocument` now projects the current decoded image frame into a compact
script snapshot for each valid image node. The persistent Canvas realm builds
a bounded image-source map, resolves canvas and image inputs through one
source adapter, and records origin cleanliness on the destination surface.
Rust validates and stores that bit beside the RGBA bytes so later snapshots
cannot silently restore a readable tainted canvas.

## Tradeoffs and follow-up

Using decoded image pixels keeps drawing deterministic and reuses the existing
HTTP(S), data-image, cache, and image-format owners, but it copies a bounded
surface across the existing script boundary. The source set is currently
canvas, `<img>`, and `ImageBitmap`; video/media sources, image
smoothing/filter breadth, color-space conversion, complete CORS-enabled image
loading, and full Canvas/Web IDL semantics remain issue #40 work. The broader
native/CDP replacement gates are unchanged.

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine canvas --locked -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`
- repository documentation coverage, depth, and release-documentation
  validators

The local Canvas witnesses pass: data-image drawing produces expected pixels,
and a different-origin HTTP image renders while readback and PNG export are
blocked with `SecurityError`. Remote CI, publication, release, and final
native/CDP parity or production-promotion claims remain pending the wider
issue #40 gates.
