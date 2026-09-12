# Native engine browser slice 226: animated WebP image playback

Status: completed locally.

## Objective

Extend the shared native image pipeline from static WebP to bounded animated
WebP playback without adding another decoder dependency, so common animated
WebP assets behave like animated GIFs in local and HTTP(S) content-process
documents.

## Contract

- `data:image/webp` and validated HTTP(S) `image/webp` responses detect
  animated WebP containers and decode their bounded frame sequence through the
  existing `image-webp` decoder.
- The logical canvas and every decoded frame are represented as RGBA pixels.
  WebP sub-rectangle offsets, alpha blending, and disposal-to-background are
  applied by the decoder before a frame is published to the shared image
  model.
- Frame durations are normalized to a non-zero bounded millisecond value.
  Finite loop counts and the infinite-loop marker are retained so completed
  finite animations remain on their last frame.
- Image and CSS background display-list commands use the shared monotonic
  animation clock, without fabricating DOM revisions, image reloads, or
  per-frame load/error events.
- The decoder rejects empty or over-limit animation sequences before frame
  publication. Canvas pixels, frame count, output buffer size, retained frame
  bytes, and content-process encoded transfer remain within the existing
  native image budgets.
- Static lossy/lossless WebP behavior, `<picture>` source selection,
  intrinsic dimensions, cache identity, current-source state, capture, and
  terminal image events remain on their existing owners.

## Implementation

`image.rs` uses the already-present `image-webp` animation API after setting
its memory limit. It validates canvas dimensions, frame count, output size,
retained-frame accounting, and loop metadata, converts opaque RGB output to
RGBA, normalizes durations, and constructs the shared `NativeImage` frame
model. No new crate is introduced. Existing `dom.rs` wire reconstruction and
`paint.rs` current-frame sampling therefore cover WebP without a second
transport or compositor path. Integration witnesses exercise local data-URL
paint and HTTP content-process paint.

## Tradeoffs and follow-up

Reusing the decoder keeps compile-time and dependency growth flat and shares
its tested WebP blend/disposal implementation. Full-canvas snapshots use more
memory than retaining encoded frames, but make display-list replay and wire
validation deterministic under the existing hard budget. The animation clock
is still process-wide and paint is sampled when a surface is requested; a
later compositor scheduler should own per-document timelines and targeted
invalidations. Color-management metadata, image-document navigation, and
complete responsive-image scheduling remain separate promotion work.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine webp -- --nocapture` (4 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine image -- --nocapture` (14 passed, 0 failed)

Implementation checkpoint: `7df8da76`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
