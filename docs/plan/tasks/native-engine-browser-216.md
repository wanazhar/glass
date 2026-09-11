# Native engine browser slice 216: image lifecycle state

Status: completed locally.

## Objective

Expose the first useful `HTMLImageElement` lifecycle state through the native
page surface. A page must be able to distinguish an image that has no source,
an image whose current resource is still pending, and an image whose current
load attempt has reached a terminal result, while retaining intrinsic
dimensions for decoded PNG resources.

## Contract

- Native `IMG` projections expose read-only `complete`, `naturalWidth`,
  `naturalHeight`, and resolved `currentSrc` properties.
- A source-less or empty-source image is complete with zero intrinsic
  dimensions and an empty `currentSrc`.
- A valid bounded data-URL PNG is complete immediately and reports its decoded
  width and height. A loaded external PNG reports the same dimensions after
  content-process hydration.
- An external image attempt records its current node/source identity before
  loading, so a successful, broken, denied, malformed, or oversized attempt
  cannot leave the page-side lifecycle state pending forever when the existing
  loader has reached a terminal result. Broken resources remain non-fatal to
  document publication.
- Assigning or removing `src` clears intrinsic dimensions and resets the
  current image state. The next document snapshot is authoritative, and stale
  node/source records are discarded after mutation.
- `currentSrc` reuses the existing URL-reflected `src` resolution against the
  active document URL. No fabricated responsive-source selection is implied.
- State is available through the local realm and the validated content-worker
  document wire; no CDP or renderer fallback is involved.

## Implementation

`dom.rs` adds bounded image-load attempt records to the typed document wire and
retains only records whose node and current `src` still match. Image snapshots
derive lifecycle values from decoded resources, data-URL decoding, and the
terminal attempt table. `content_process.rs` records external image attempts
before the existing policy/cache/decoder path and refreshes state around script
mutation hydration. `javascript.rs` installs the image Web IDL properties,
keeps local setters coherent, and refreshes the values from each authoritative
snapshot. Integration witnesses cover local data/empty images and external
PNG dimensions/current-source reporting.

## Tradeoffs and follow-up

This is a bounded lifecycle owner, not complete image compatibility. It does
not add `srcset`/`sizes` candidate selection, `picture` source selection,
`decode()`, `loading`, `decoding`, `fetchPriority`, image error-event parity,
SVG/animated/modern image formats, CSS image layers, or browser-wide Web IDL
descriptor conformance. The state table stores one current source per node,
which keeps wire validation and mutation invalidation deterministic; a future
resource scheduler can extend it to concurrent candidates without changing the
host-facing property names. The current external hydration remains serialized
with document publication, so the broader scheduler/performance work remains
open.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_image_element_exposes_complete_intrinsic_dimensions_and_current_src -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_external_png_through_document_wire -- --nocapture` (1 passed, 0 failed)

Implementation checkpoint: `13ebf028`.

The next issue #40 work must expand image compatibility and resource/event
ownership while preserving the native-only boundary; browser-complete
promotion still requires the full profile, security, platform, integration,
documentation, and release gates.
