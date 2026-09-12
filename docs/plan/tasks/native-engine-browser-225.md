# Native engine browser slice 225: animated GIF image playback

Status: completed locally.

## Objective

Extend the shared native image pipeline from static GIF to bounded animated
GIF playback so ordinary images and CSS background images can advance frames
through the native compositor in local and HTTP(S) content-process documents.

## Contract

- `data:image/gif` and validated HTTP(S) `image/gif` responses decode a
  bounded sequence of GIF frames into full logical-canvas RGBA snapshots.
- Sub-rectangle frames are composited in order. Transparent source pixels
  preserve the prior canvas, and the supported GIF disposal modes retain,
  clear, or restore the appropriate canvas state before the next frame.
- Frame delays are normalized to a non-zero bounded millisecond value. GIF
  finite-repeat and infinite-repeat metadata is retained so completed finite
  animations remain on their final frame rather than restarting indefinitely.
- Image and CSS background display-list commands sample the current frame from
  a monotonic animation clock. Frame advancement does not fabricate DOM
  revisions, resource reloads, or per-frame load/error events.
- Content-process image wire carries the frame sequence and loop metadata;
  parent-side validation requires exact RGBA dimensions, current-frame
  identity, valid delays, bounded frame count, bounded decoded bytes, and a
  bounded encoded payload before publication.
- Malformed, out-of-bounds, zero-delay, over-limit, or inconsistent animated
  GIF input fails closed as a broken image without aborting the containing
  document or creating an unbounded frame allocation.

## Implementation

`image.rs` adds bounded GIF frame iteration, logical-canvas compositing,
transparent-pixel handling, disposal restoration, delay normalization, loop
metadata, and time-based current-frame selection. Static image constructors
remain compatible through the shared `NativeImage` representation. `paint.rs`
uses the current frame for both replaced-image and CSS background commands.
`dom.rs` extends the typed resource wire and centralizes parent validation so
local decoding and content-process reconstruction enforce the same contract.
The integration witnesses cover local animated data-URL paint and animated
HTTP image paint after the content-process transfer.

## Tradeoffs and follow-up

Retaining full-canvas snapshots makes paint deterministic and keeps raster
replay simple, at the cost of bounded memory proportional to frame count and
canvas area. A process-wide monotonic clock keeps this slice independent of
DOM revisions and event queues, but a later compositor scheduler should own
per-document animation timelines and invalidate only affected surfaces. GIF
support now has a native frame/compositing path; animated WebP/APNG, color
profiles, image-document navigation, and complete responsive-image scheduling
remain separate format and scheduling work.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine gif -- --nocapture` (4 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine image -- --nocapture` (13 passed, 0 failed)
- Scoped strict Clippy was attempted; it remains blocked by 111 pre-existing
  crate-wide warnings outside this slice. The changed slice has no new
  warning beyond existing line-shifted diagnostics in `image.rs`, `dom.rs`,
  `paint.rs`, and the integration test.

Implementation checkpoint: `1b7da97f`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
