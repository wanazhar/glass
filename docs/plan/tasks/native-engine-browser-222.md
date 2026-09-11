# Native engine browser slice 222: JPEG image decode

Status: completed locally.

## Objective

Make common JPEG resources first-class native images. Data URLs and external
HTTP(S) responses must use one bounded decode path and feed the image lifecycle
already shared by intrinsic sizing, `currentSrc`, picture selection, paint,
capture, cache, and load/error events.

## Contract

- `data:image/jpeg` and HTTP(S) `image/jpeg` responses are decoded to RGBA
  pixels. Both baseline and progressive JPEG coding are accepted, including
  the progressive real-world fixture that the previous decoder rejected.
- The decoder validates dimensions and the final RGBA byte count before
  retaining pixels. External resources remain bounded by the existing
  `MAX_NATIVE_IMAGE_TRANSFER_BYTES` transfer limit; inline resources use the
  existing larger native image bound.
- Progressive decoding is capped at 64 scans to bound CPU/memory exposure.
  Images with invalid markers, unsupported precision/color output, malformed
  data, or over-limit decoded output become non-fatal broken-image results.
- JPEG pixels use the same intrinsic/aspect-ratio sizing, typed display-list
  image command, nearest-neighbor software replay, clipping, scrolling, hit
  testing, capture, cache, and terminal load/error event owners as PNG pixels.
- `<picture><source type="image/jpeg">` is a supported source type and uses
  the existing bounded media/order/`srcset`/`sizes` selection contract.
  Unsupported declared formats remain ineligible rather than being fetched by
  a decoder that cannot validate them.
- The HTTP loader advertises common image formats for negotiation but only
  accepts formats with a native decoder. Content type is checked before bytes
  are decoded and before cache insertion.

## Implementation

`image.rs` retains the existing PNG decoder and adds a pure-Rust `zune-jpeg`
decoder configured for RGBA output, safe dimensions, and a progressive scan
cap. The decoder checks headers and output size before allocating the final
pixel buffer. `resource_loader.rs` accepts `image/jpeg`, decodes the response,
and stores the same requested/final URL cache keys used by PNG. Data-image
dispatch and picture type matching share the same supported-format contract.

## Tradeoffs and follow-up

The added decoder is a small direct dependency with architecture optimizations
disabled in this first integration, so compilation and some decode workloads
cost more than the previous PNG-only build. That cost buys valid progressive
JPEG behavior without a system library or a hidden browser backend. WebP,
GIF/APNG animation, AVIF, SVG image documents, ICC/color-management fidelity,
EXIF orientation, and full responsive-image scheduling remain later decoder
and rendering slices; they are explicit work rather than silently treated as
JPEG/PNG.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --lib image::tests::decode_progressive_jpeg_fixture -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_jpeg_data_images_expose_intrinsic_dimensions_and_paint -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_jpeg_picture_source -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_selects_picture_source_and_reloads_img_fallback -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_images_ -- --nocapture` (2 passed, 0 failed)

Implementation checkpoint: `a94208ad`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
