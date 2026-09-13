# Glass native engine browser slice 315: animated Canvas image sources

Status: completed locally; issue #40 production parity and remote CI remain
open.

## Objective

Carry the native image decoder's bounded animation state into the persistent
page JavaScript Canvas source adapter. A page-held animated `<img>` must keep
its decoded frames and select the active frame when `drawImage()` is called,
instead of becoming a one-frame bitmap after the first host bootstrap.

## Contract

- Page image snapshots retain the existing decoded RGBA frame limit and carry
  each validated frame's pixels and delay, the optional loop count, and the
  sampled monotonic animation time used for the snapshot.
- The page Canvas image-resource map reconstructs frame buffers only when they
  pass the existing dimensions and byte limits. Malformed optional frame
  metadata is ignored without turning a valid current image into an invalid
  resource.
- `drawImage()` selects the active frame from the snapshot's animation origin,
  honors infinite and finite loop behavior, and holds the final frame after a
  finite animation completes. `ImageBitmap` creation and other Canvas source
  consumers receive the selected frame through the same source adapter.
- The Rust paint/compositor path remains the authoritative visual owner for
  ordinary image presentation; this slice makes page-realm Canvas reads use
  the same frame timing rather than introducing a second decoder.
- Static images retain the previous single-buffer path and incur no animation
  frame scheduling or timing state.

## Path

- `crates/glass-browser/src/browser/native_engine/image.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

`NativeImage` exposes its sampled animation clock to the script snapshot.
`NativeScriptImageResourceSnapshot` now carries bounded frame snapshots and
loop metadata. The Canvas bootstrap validates and decodes those optional
buffers, derives a page-local animation origin from the Rust sample, and
selects a frame for each Canvas source read. The map remains refreshed in
place, preserving the persistent DOM/context closure behavior established by
the OffscreenCanvas slice.

## Tradeoffs and follow-up

The bridge remains snapshot-based at host turns and uses the existing
software/decoded RGBA owners; it does not add a separate background animation
thread, video/Audio/MediaSource decoding, frame callbacks, exact browser task
timing, or new image formats. The content-process wire already carries the
same bounded frame metadata, so no second network or decoder path is created.
Complete image/Web IDL semantics, CORS behavior, worker-realm Canvas, video
and media elements, and final native/CDP replacement remain issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine canvas --locked -- --nocapture`
- `git diff --check`
- repository documentation coverage, depth, and release-documentation
  validators

The focused Canvas filter passes locally with 3 tests. The added witness
verifies animated frame count/delays, deterministic first and final frame
selection, image readiness, and intrinsic dimensions through page
`drawImage()`. Remote CI, publication, release, and final native/CDP parity
or production-promotion claims remain pending the wider issue #40 gates.
