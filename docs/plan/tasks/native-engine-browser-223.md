# Native engine browser slice 223: static WebP image decode

Status: completed locally.

## Objective

Extend common image compatibility from PNG/JPEG to static WebP while keeping
one bounded decode, cache, document-wire, paint, and lifecycle owner. WebP
resources must work for data URLs, external HTTP(S) images, and picture source
selection without silently using CDP or an unvalidated first-frame shortcut.

## Contract

- `data:image/webp` and HTTP(S) `image/webp` responses decode lossy and
  lossless non-animated WebP into RGBA pixels. Alpha-bearing images preserve
  their decoded alpha channel; opaque images receive an opaque alpha channel
  for the shared renderer format.
- Dimensions, decoded output size, and retained pixel count are checked before
  allocation/transfer. External images keep the existing
  `MAX_NATIVE_IMAGE_TRANSFER_BYTES` bound; inline images keep the existing
  native image bound.
- Animated WebP is rejected as a broken image until the native engine has a
  frame clock, frame selection, timing events, and repaint invalidation owner.
  The engine therefore never presents a first frame as complete playback.
- Static WebP pixels share PNG/JPEG intrinsic/aspect-ratio sizing, typed
  display-list paint, nearest-neighbor replay, clipping, scrolling, hit
  testing, capture, cache, `currentSrc`, and terminal load/error behavior.
- `<picture><source type="image/webp">` is eligible when its bounded media
  and source-selection rules choose it. Declared formats without a decoder
  remain ineligible and do not trigger a speculative network request.
- HTTP content type is validated before decode and cache insertion. Malformed,
  unsupported, oversized, or animated WebP results remain non-fatal to the
  surrounding document.

## Implementation

`image.rs` adds the pure-Rust `image-webp` decoder. It validates dimensions
against the existing RGBA limits, rejects animation, allocates only the
decoder-reported RGB/RGBA output, and normalizes opaque output to RGBA.
`resource_loader.rs` admits `image/webp` through the same redirect, CSP,
mixed-content, referrer, cookie, cache, and content-type path as PNG/JPEG.
`dom.rs` shares the supported-format registry with picture type matching.

## Tradeoffs and follow-up

The direct decoder adds a small dependency and decode cost, but remains pure
Rust and avoids a system library or a hidden browser backend. Animation needs
more than a decoder: it requires deterministic frame timing, resource memory
limits across frames, repaint scheduling, and media/image event ordering.
GIF/APNG animation, AVIF, SVG image documents, ICC/color-management fidelity,
EXIF orientation, and complete responsive-image scheduling remain explicit
promotion work.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_webp_data_images_expose_intrinsic_dimensions_and_paint -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_webp_picture_source -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_jpeg_picture_source -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_dispatches_external_image_error_event -- --nocapture` (1 passed, 0 failed)

Implementation checkpoint: `849ed441`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
