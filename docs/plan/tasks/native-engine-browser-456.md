# Native engine browser-complete slice 456: Blob image subresources

- Status: complete locally
- Scope: `native-engine` / process-backed page image consumers
- Issue: #40
- Depends on: [native-engine-browser-455](native-engine-browser-455.md)

## Objective

Allow a page-created Blob URL to serve generic page image consumers after a
script mutation. The content-process owner must use the live page registry
only to take a bounded snapshot and must keep ordinary HTTP(S) image loading
unchanged.

## Contract

- A runtime-verified page Blob URL can populate an `<img>` source and a CSS
  `background-image` source in a process-backed page.
- The existing supported image MIME set and bounded image transfer/decoded
  pixel limits apply to the Blob bytes.
- Successful image loads update intrinsic dimensions and paint resources; the
  normal load event path is preserved. Unsupported MIME or decode failure
  remains an image error, not a network fallback.
- The document CSP image policy is evaluated before Blob decoding: an explicit
  `img-src blob:` source can admit the resource, while `img-src 'none'` blocks
  it and preserves the normal error path.
- Blob image reads are not inserted into the HTTP image cache, so revocation or
  registry lifetime cannot be bypassed by a stale cache entry.
- A missing or revoked registry entry remains unavailable. Non-Blob URL
  behavior continues through the existing HTTP(S) policy, redirect, cookie,
  cache, and CSP path.

## Implementation

- Added a Blob-aware image loader entry point that validates the resolved
  `blob:` URL, creator-origin syntax, CSP image policy, Blob MIME type, and
  native decoder limits.
- Snapshot page object-URL bytes from the content-process JavaScript runtime
  while processing dynamic image and background-image sources.
- Kept initial document loading on the existing loader path; the registry is
  consulted only when a live page runtime is available for a mutation turn.
- Reused the existing document image state, event dispatch, display-list, and
  rasterization owners for both element and background images.

## Tradeoffs and remaining scope

This is a bounded copy from the page registry into the image decoder. It does
not claim zero-copy sharing or a cross-process live Blob registry. The slice is
limited to process-backed page mutations; Blob stylesheet, classic/module
script, media, popup/window transfer, local-inline dynamic image ownership,
and broader cross-realm sharing remain separate issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_loads_blob_object_url_image_subresources -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_blocks_csp_disallowed_blob_image_subresources -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked blob_object_urls -- --nocapture`
- `git diff --check`
