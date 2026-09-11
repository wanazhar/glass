# Native engine browser slice 211: external PNG image resources

Status: completed locally.

## Objective

Make static external PNG images participate in the same native content-owner,
document-wire, layout, display-list, software-raster, hit-test, and capture
path as inline data-URL images.

## Contract

- The content process discovers bounded `<img src>` resources that are not
  `data:` URLs during an HTTP(S) document load. The existing resource loader
  resolves relative URLs and keeps the request inside the native HTTP(S)
  policy owner.
- Image requests apply the existing credential rejection, redirect limit,
  referrer, cookie, mixed-content, and document CSP `img-src` decisions. Only
  successful `image/png` responses within the bounded transfer limit are
  decoded; a denied, malformed, unsupported, failed, or oversized image is a
  non-fatal broken-image result for the page.
- PNG decoding expands supported color forms into validated RGBA pixels. The
  content process transfers the node index, original `src`, intrinsic width and
  height, and bounded pixels through the typed document wire. The parent
  rejects stale, duplicate, out-of-range, mismatched, malformed, or
  over-budget image resources before publication.
- Transferred image dimensions participate in intrinsic sizing and declared
  width/height aspect-ratio resolution. The parent display list paints the
  transferred pixels through the existing nearest-neighbor, source-over,
  clipping, scroll, hit-testing, and PNG-capture owners.
- A successfully loaded external image receives the existing typed `load`
  event before document readiness events. The initial static image set is the
  scope of this slice; later DOM-created or `src`-changed images require a
  resource invalidation/refetch path.

## Implementation

`native_engine::resource_loader` now owns bounded external PNG fetch and
decode. `NativeDocumentWire` carries validated image resources, while
`NativeDocument` checks that each resource still belongs to its current
`img` source before retaining or serializing it. Layout and paint consult that
resource map before the inline data-URL decoder. The content worker loads
images before page scripts, adds successful image nodes to the existing load
event queue, and keeps resource failures from aborting an otherwise valid
document.

## Tradeoffs and follow-up

The first external format is PNG so the existing dependency can be reused and
the parent receives deterministic RGBA pixels without a second decoder or
renderer. The transfer cap is intentionally lower than the inline-data cap to
bound content-process IPC and parent allocation; raw RGBA is base64-encoded in
the already bounded document snapshot, which trades wire size for simple
validation. Requests are currently an initial-load batch without decoded
image caching, freshness/revalidation, responsive `srcset`/`picture`, CSS
image paints, SVG image resources, animation, color management, or other
modern image formats. Those remain active issue #40 browser-completeness work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_external_png_through_document_wire -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_blocks_csp_disallowed_image_before_request -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_inline_ -- --nocapture` (3 passed, 0 failed)

Implementation checkpoint: `ced45e4a`.
