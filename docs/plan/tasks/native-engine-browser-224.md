# Native engine browser slice 224: static GIF image decode

Status: completed locally.

## Objective

Extend the shared native image pipeline from PNG/JPEG/WebP to static GIF so
ordinary pages and responsive `<picture>` sources can load and paint GIF images
without delegating to CDP or presenting an unowned animation shortcut.

## Contract

- `data:image/gif` and validated HTTP(S) `image/gif` responses decode one
  complete full-canvas GIF frame into the shared RGBA image representation.
- The logical canvas must have non-zero dimensions within the existing native
  pixel budget. Decoder memory, decoded byte length, and external transfer
  limits are enforced before image publication.
- A GIF with another frame or a first frame that is only a sub-rectangle of the
  logical canvas is rejected as a broken image. No first-frame-only animation
  claim is made before native frame timing, disposal/compositing, and repaint
  invalidation exist.
- Static GIF pixels share the existing intrinsic sizing, `currentSrc`, cache,
  typed document wire, display-list paint, software replay, clipping, capture,
  and terminal load/error behavior.
- `<picture><source type="image/gif">` participates in the existing bounded
  media, `srcset`, `sizes`, source-order, and fallback selection contract.
- Malformed, unsupported, animated, sub-rect, and over-limit GIF input fails
  closed without aborting the containing document or creating a speculative
  request for an unsupported picture source.

## Implementation

`image.rs` adds the pure-Rust `gif` decoder with RGBA output, decoder memory
limits, frame-consistency validation, logical-canvas bounds, and exact output
length checks. `resource_loader.rs` admits `image/gif` through the existing
redirect, CSP, mixed-content, referrer, cookie, cache, content-type, and
external-transfer path. `dom.rs` adds GIF to the picture type registry. The
integration witnesses exercise both data-URL paint and HTTP content-process
picture loading.

## Tradeoffs and follow-up

The small pure-Rust decoder adds a dependency and a bounded decode cost, but
keeps image behavior deterministic and avoids a native system library. Static
GIF support covers common icons and legacy assets while deliberately refusing
to misrepresent animated content. Animation requires a frame clock, delay and
loop policy, disposal/compositing state, per-frame memory accounting, repaint
invalidation, and image event ordering. GIF transparency/color management,
interlaced scheduling, APNG/animated WebP, AVIF, SVG image documents, and full
responsive-image scheduling remain separate promotion work.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_gif_data_images_expose_intrinsic_dimensions_and_paint -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_gif_picture_source -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine image -- --nocapture` (12 passed, 0 failed)

Implementation checkpoint: `3c0c37bb`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
