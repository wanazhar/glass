# Native engine browser slice 227: animated APNG image playback

Status: completed locally.

## Objective

Extend the shared native image pipeline to animated PNG (APNG), including
subframe composition and timing, so `image/apng` assets use the same native
image lifecycle as GIF and WebP in local and HTTP(S) content-process pages.

## Contract

- `data:image/apng`, `data:image/png` APNG containers, and validated HTTP(S)
  `image/apng` or `image/png` responses detect APNG animation through the
  existing PNG decoder.
- Each APNG frame is decoded as a bounded raw subframe and composed onto the
  logical canvas using its x/y offset, source or over blend operation, and
  none/background/previous disposal operation.
- Frame delays treat a zero denominator as 100 and normalize zero or
  sub-millisecond values to a non-zero bounded millisecond value. The APNG
  finite-repeat count and infinite-loop marker are retained in the shared
  animation model.
- Image and CSS background display-list commands sample the current full-canvas
  frame from the shared monotonic animation clock. Frame progression does not
  create DOM revisions, reload the resource, or emit per-frame load/error
  events.
- The decoder validates the logical canvas, frame count, output buffer, exact
  frame dimensions, retained full-canvas bytes, and typed content-process
  payload before publication. Malformed, inconsistent, and over-limit input
  fails closed without aborting the document.
- Static PNG behavior and existing GIF/WebP animation behavior retain their
  intrinsic sizing, cache identity, source selection, capture, and terminal
  resource-event owners.

## Implementation

`image.rs` reuses `png::Reader` with the existing expand/strip-16
transformations. APNG metadata is inspected before decode; optional separate
default images are discarded, then bounded frame controls and raw subframes
are composed into full-canvas RGBA snapshots. The compositor implements
source replacement, alpha-over blending, transparent clearing, and previous
canvas restoration, with APNG timing and loop metadata converted to the shared
`NativeImage` representation. `image/apng` is admitted in data URLs, HTTP
content-type validation, and `<picture>` type selection. No new dependency is
introduced. Existing typed-wire and paint owners carry the result.

## Tradeoffs and follow-up

Full-canvas snapshots use bounded memory proportional to frame count and
canvas size, but make disposal semantics, display-list replay, and
content-process validation deterministic. The current process-wide monotonic
clock is deliberately simple; a production compositor scheduler should later
own per-document timelines and targeted invalidations. Color management,
image-document navigation, and complete responsive-image scheduling remain
separate promotion work.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine apng -- --nocapture` (2 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine image -- --nocapture` (15 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine content_process_paints_animated -- --nocapture` (3 passed, 0 failed)

Implementation checkpoint: `489da8f2`.

The broader Issue #40 browser profile and native-only promotion gates remain
active work.
