# Glass native engine browser slice 312: ImageBitmap surfaces

Status: completed locally; issue #40 production parity and remote CI remain
open.

## Objective

Make decoded image and Canvas 2D pixels available through the page's native
`ImageBitmap` source path. A bitmap created by script must be a real bounded
drawable surface with the lifecycle and readback security state expected by
Canvas consumers.

## Contract

- The page realm exposes an `ImageBitmap` constructor identity. Native bitmap
  objects pass `instanceof ImageBitmap`, expose `width` and `height`, and
  release their surface through idempotent `close()`; direct construction is
  rejected.
- `createImageBitmap()` returns a Promise and accepts the supported image,
  canvas, ImageData, and ImageBitmap sources. It supports the no-crop,
  crop-rectangle, and bounded resize forms used by the native Canvas path.
- Crop and resize operations validate finite dimensions and the existing
  native axis/area limits, sample deterministically, and preserve the source
  origin-clean bit.
- Canvas 2D `drawImage()` accepts an open ImageBitmap through the existing
  transformed source sampler. Drawing a closed bitmap fails with
  `InvalidStateError` and does not mutate the destination.
- Bitmap creation and drawing remain inside the existing bounded page-turn
  surface. Origin-unclean source state continues to make destination pixel and
  PNG readback raise `SecurityError`.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The persistent Canvas realm now owns a small native `ImageBitmap` wrapper whose
private surface points at a bounded RGBA copy. The shared Canvas source adapter
recognizes open bitmaps alongside canvases and decoded images. The page
`createImageBitmap()` implementation normalizes supported source and crop
forms, performs bounded nearest-neighbor crop/resize sampling, carries forward
origin cleanliness, and resolves the resulting bitmap through a Promise. The
existing per-turn Canvas commit and readback guards therefore remain the
authority for raster persistence and security.

## Tradeoffs and follow-up

This slice keeps bitmap creation deterministic and dependency-free by copying
bounded RGBA pixels in the page realm. It intentionally does not claim
`Blob`/`File`, video/VideoFrame, SVG, decode metadata,
`imageOrientation`, `premultiplyAlpha`, `colorSpaceConversion`,
`resizeQuality`, transfer/structured-clone ownership, color management, or
complete ImageBitmap/Web IDL descriptor semantics. Those sources and the wider
Canvas, media, rendering, conformance, and native/CDP replacement gates remain
issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine canvas --locked -- --nocapture`
- `git diff --check`
- repository documentation coverage, depth, and release-documentation
  validators

The focused Canvas filter passes locally. It covers ImageBitmap identity,
Promise-backed crop/resize creation, Canvas drawing, closed-bitmap lifecycle
errors, existing canvas/image persistence, and cross-origin readback taint.
Remote CI, publication, release, and final native/CDP parity or production-
promotion claims remain pending the wider issue #40 gates.
