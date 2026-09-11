# Native engine browser slice 217: image error events

Status: completed locally.

## Objective

Make failed external image loads observable through the native event surface.
An image that cannot be fetched, is denied by policy, has an unacceptable
response, or cannot be decoded must reach a terminal page-visible outcome
without aborting document publication or silently disappearing.

## Contract

- External image discovery and script-driven `src` hydration emit a typed
  terminal outcome: `load` for a decoded PNG resource or `error` for a failed
  attempt.
- The native `error` event is dispatched through the existing validated host
  event bridge, is non-bubbling, and is non-cancelable. Event listeners see
  `event.type === "error"` on the affected `IMG` target.
- Initial page-load errors are delivered after page scripts have installed
  listeners and before the native page-load event sequence completes.
- A script replacement that fails produces one error event for that attempt;
  the current `src`, `complete`, and zero intrinsic dimensions remain visible
  through the authoritative image lifecycle snapshot.
- Failed, denied, malformed, unsupported, oversized, and transport-error
  image attempts remain non-fatal to the document and do not create decoded
  image paint resources.
- The same event-kind validation applies to content-process response decoding
  and same-origin frame event metadata. No CDP or renderer fallback is used.

## Implementation

`NativeEventKind::Error` is carried through host/frame event serialization and
content-process response decoding. Initial resource discovery now returns
typed resource events instead of only successful node IDs, allowing styles,
scripts, images, and failed images to share the existing post-script dispatch
sequence. Script mutation hydration emits the corresponding `load` or `error`
event for each new external image attempt. The image lifecycle state from slice
216 records the attempt before loading, so every terminal failure is reflected
as `complete` without a fabricated decoded resource.

## Tradeoffs and follow-up

This slice covers the native external PNG owner only. It does not add image
`onerror` handler attributes, `srcset`/`sizes` candidate selection,
`picture` source selection, `decode()`, SVG/animated/modern image formats,
resource cancellation/coalescing, or complete browser event-loop ordering.
Resource attempts remain serialized with document publication, and an error
event carries no detailed network/decoder reason to page script, matching the
privacy boundary of the existing loader.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_dispatches_external_image_error_event -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_external_png_through_document_wire -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_external_background_png_through_document_wire -- --nocapture` (1 passed, 0 failed)

Implementation checkpoint: `f45ae4e5`.

Further image compatibility, page event-loop semantics, broader subresource
classes, and the complete Issue #40 profile remain active promotion work.
