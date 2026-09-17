# Native engine browser-complete slice 461: local Blob stylesheets

- Status: complete locally
- Scope: `native-engine` / local non-network page dynamic stylesheets
- Issue: #40
- Depends on: [native-engine-browser-460](native-engine-browser-460.md)

## Objective

Allow a local `fixture:` or other non-network page to attach a stylesheet
whose `href` is owned by the page's Blob object-URL registry. The local owner
must update CSS state once, preserve bounded link attempt state, and deliver
the normal stylesheet resource event.

## Contract

- A dynamically attached stylesheet with a runtime-owned `blob:` URL is loaded
  without network or HTTP-cache transport.
- Blob stylesheet MIME, source-size, UTF-8, and recognized SRI checks remain
  enforced; unavailable, revoked, blocked, or invalid entries dispatch
  `error` and have no CSS effect.
- A successful stylesheet rebuilds native CSS and background-image source
  state before dispatching `load`.
- A link with unchanged node and `href` retains its prior result and is not
  retried on unrelated local turns; removed or changed links receive a fresh
  bounded attempt.
- HTTP(S) and process-backed stylesheet ownership remains unchanged.

## Implementation

- Added a bounded synchronous local Blob stylesheet loader path.
- Added local per-link stylesheet attempt state using the existing document
  stylesheet owner and rebuild operation.
- Routed local Blob stylesheet events through the existing native event bridge.

## Tradeoffs and remaining scope

Stylesheet bytes are copied into local document state and are not a live or
zero-copy view of the JavaScript Blob registry. Local Blob images/modules,
media consumers, popup/window transfer, and cross-realm object-URL sharing
remain separate issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked native_local_dynamic_blob_stylesheet_updates_style_after_late_attachment -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_local_dynamic -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_loads_blob_object_url_stylesheet_subresources -- --nocapture`
- `git diff --check`
