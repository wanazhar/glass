# Native engine browser-complete slice 458: Blob stylesheet subresources

- Status: complete locally
- Scope: `native-engine` / process-backed page dynamic stylesheets
- Issue: #40
- Depends on: [native-engine-browser-457](native-engine-browser-457.md)

## Objective

Allow a page-created Blob URL to serve a dynamically inserted
`<link rel="stylesheet">` in an HTTP(S) content process. The content owner
must retain bounded loaded/failed link state, update the native CSS owner, and
preserve the ordinary stylesheet resource-event contract.

## Contract

- A runtime-verified page Blob URL can populate a dynamic stylesheet link.
- Document style CSP is evaluated before Blob CSS admission. A disallowed
  source produces the normal link `error` event and no CSS effect.
- Blob CSS content type, maximum source size, UTF-8 decoding, and
  subresource-integrity checks remain enforced.
- A successful Blob stylesheet updates computed style and related background
  image-source discovery, then delivers the link `load` event.
- Blob stylesheet reads bypass HTTP transport and stylesheet-cache lookup;
  missing or revoked registry entries remain unavailable.
- A link’s failed or successful attempt is retained while its node and
  `href` remain unchanged, preventing duplicate network/registry work on
  unrelated script turns. Removed links are pruned, and changed links get a
  fresh bounded attempt.
- Existing HTTP(S), parser-inserted, redirect, cache, inline-style policy,
  resource-order, and parent wire behavior remains on its existing owner path.

## Implementation

- Added a Blob-aware stylesheet loader entry point with origin, CSP, MIME,
  size, SRI, and UTF-8 validation before the existing HTTP/cache path.
- Added bounded per-link stylesheet attempt state to the content-process
  document and rebuilt the native stylesheet plus background-image source
  map only when live link state changes.
- Loaded dynamic links after script mutations and dispatched their normal
  target event through the persistent page runtime.

## Tradeoffs and remaining scope

Stylesheet bodies are copied from the page registry into the content-process
CSS owner and kept only within bounded document state; this is not a
zero-copy or live cross-process registry. CSS import dependency graphs are
not added by this slice. Module dependency graphs, media, popup/window
transfer, local-inline dynamic consumers, and broader cross-realm object-URL
sharing remain separate issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_loads_blob_object_url_stylesheet_subresources -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_blocks_csp_disallowed_blob_stylesheet_subresources -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_loads_same_origin_stylesheet_under_csp -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_blocks_csp_disallowed_stylesheet_before_request -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_revalidates_stylesheet_and_script_subresources -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_dispatches_resource_load_events_before_dom_content_loaded -- --nocapture`
- `git diff --check`
