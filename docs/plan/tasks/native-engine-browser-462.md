# Native engine browser-complete slice 462: local Blob images

- Status: complete locally
- Scope: `native-engine` / local non-network page dynamic images
- Issue: #40
- Depends on: [native-engine-browser-461](native-engine-browser-461.md)

## Objective

Allow a local `fixture:` or other non-network page to attach an image whose
source is owned by the page's Blob object-URL registry. The local owner must
decode the image once, expose the normal intrinsic image state, update both
element and CSS background paint consumers, and deliver the image resource
event.

## Contract

- A dynamically attached `<img>` with a runtime-owned `blob:` URL is loaded
  without network or HTTP-cache transport.
- A runtime-owned `blob:` URL used by `background-image` is resolved through
  the same bounded image decoder and paint-resource owner.
- Blob image MIME admission, decode limits, intrinsic dimensions, and pixel
  bounds remain enforced; unavailable, revoked, unsupported, or invalid
  entries dispatch image `error` for `<img>` and have no paint effect.
- A successful `<img>` load updates `complete`, `naturalWidth`,
  `naturalHeight`, `currentSrc`, and the native display-list resource before
  its `load` event is delivered.
- Repeated local turns do not reload an unchanged successful image resource;
  the existing native source ownership and node-generation checks remain in
  force.
- HTTP(S) and process-backed image ownership remains unchanged.

## Implementation

- Added a bounded synchronous local Blob image loader that reuses the native
  image decoder and refuses network/cache fallback.
- Added local dynamic Blob image discovery for `<img>` and CSS
  `background-image` consumers.
- Routed local image `load`/`error` events through the existing native event
  bridge and preserved the existing display-list paint path.

## Tradeoffs and remaining scope

Decoded pixels are copied into native image state for the local turn; the
runtime registry remains the lifetime authority and this is not a zero-copy
claim. Local media, popup/window transfer, and cross-realm object-URL sharing
remain separate issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked native_local_dynamic_blob_images_load_and_paint_after_late_attachment -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_local_dynamic -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_loads_blob_object_url_image_subresources -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_blocks_csp_disallowed_blob_image_subresources -- --nocapture`
- `git diff --check`
