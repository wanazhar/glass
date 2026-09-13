# Glass native engine browser slice 313: OffscreenCanvas surfaces

Status: completed locally; issue #40 production parity and remote CI remain
open.

## Objective

Extend the retained Canvas 2D surface to the page-facing OffscreenCanvas
workflow. A placeholder HTML canvas must be able to transfer its drawing
control to an OffscreenCanvas, while standalone page OffscreenCanvas objects
remain useful for local rendering, bitmap snapshots, and bounded export.

## Contract

- The page realm exposes a constructable `OffscreenCanvas` with bounded,
  integer `width` and `height` properties, `getContext('2d')`,
  `transferToImageBitmap()`, and Promise-backed `convertToBlob()`.
- `HTMLCanvasElement.transferControlToOffscreen()` returns an OffscreenCanvas
  sharing the placeholder's retained native surface. A placeholder cannot
  acquire a second context, transfer twice, or use `toDataURL()`/`toBlob()`
  after transfer; these cases raise `InvalidStateError`.
- Drawing through a transferred OffscreenCanvas commits to the same native
  canvas resource used by layout, screenshots, and Canvas `drawImage()`.
  Standalone OffscreenCanvas drawing remains realm-local and emits no invalid
  native-node commit.
- Offscreen dimension changes reset pixels and drawing state, synchronize
  transferred placeholder attributes, preserve the existing axis/area limits,
  and rehydrate correctly after a host snapshot refresh.
- `transferToImageBitmap()` copies the bounded RGBA surface and preserves its
  origin-clean state; `convertToBlob()` uses the existing PNG and security
  guards.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The Canvas source/context adapters now accept both HTML and OffscreenCanvas
backing surfaces. Transferred placeholders expose a shared surface getter and
native node index, while standalone objects own bounded local surfaces. The
shared resource maps are refreshed in place on every host bootstrap so older
DOM/context closures cannot rehydrate from an obsolete blank map. Offscreen
dimension setters reset state and emit a bounded attribute mutation when a
placeholder is linked; transfer-to-bitmap and PNG export reuse the existing
copy and origin-clean owners.

## Tradeoffs and follow-up

This slice keeps the implementation deterministic and software-raster based;
it does not claim worker-realm OffscreenCanvas installation, WebGL/WebGL2 or
WebGPU contexts, video/VideoFrame/media sources, display-pipeline color
management, transfer/structured-clone ownership, or complete OffscreenCanvas,
Canvas, and Web IDL descriptor semantics. Those capabilities and the wider
rendering, conformance, and native/CDP replacement gates remain issue #40
work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine canvas --locked -- --nocapture`
- `git diff --check`
- repository documentation coverage, depth, and release-documentation
  validators

The focused Canvas filter passes locally with 2 tests. It covers transferred
and standalone OffscreenCanvas identity, shared placeholder presentation,
ImageBitmap snapshots, PNG export, transfer-state errors, retained Canvas
pixels across a host refresh, and cross-origin readback taint. Remote CI,
publication, release, and final native/CDP parity or production-promotion
claims remain pending the wider issue #40 gates.
