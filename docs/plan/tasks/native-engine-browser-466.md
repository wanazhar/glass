# Native engine browser-complete slice 466: cross-realm MessagePort Blob transfer

- Status: complete locally
- Scope: `native-engine` / MessagePort structured-message Blob URL ownership
- Issue: #40
- Depends on: [native-engine-browser-465](native-engine-browser-465.md)

## Objective

Make a Blob URL referenced by a structured message sent through a transferred
`MessagePort` usable in the destination realm. Page, dedicated/shared-worker,
popup/`WindowProxy`, and Service Worker port delivery must share one bounded
browser-owned resource envelope across inline and content-process owners.

## Contract

- A page-, worker-, popup-, or Service Worker-owned Blob URL embedded at any
  supported bounded depth in a MessagePort payload is snapshotted before the
  message leaves its source realm and is available to the destination
  Fetch/XHR owner after delivery.
- Active-target, parked-target, native-frame, and HTTP(S) content-process
  MessagePort routes carry the same typed transfer envelope; messages without
  transfers remain compatible.
- The destination installs every bounded snapshot before structured-clone
  decoding. The original URL string is preserved, while source-realm
  revocation remains independent from the destination snapshot.
- Transfer count, URL, MIME, byte, nesting, aggregate-message, and
  content-process framing limits remain enforced; malformed transfers fail
  closed without network or cache fallback.
- Same-realm MessagePort cloning continues to use the existing local clone
  path; this slice adds the explicit snapshot only at a realm or process
  boundary.

## Implementation

- Added an `object_urls` transfer list to MessagePort commands, page events,
  and page-owned content-process commands, with serde defaults for older
  frames.
- Captured Blob URL resources in the active QuickJS context, validated the
  bounded envelope at engine, content-process, and Service Worker boundaries,
  and propagated it through worker, popup, frame, parked-target, and
  service-worker routes.
- Installed destination snapshots in the shared MessagePort bridge before
  clone decoding, covering both bridge-ID and bridge-key dispatch.
- Reused the existing typed Blob snapshot owner and limits; no live JavaScript
  object, network lookup, or HTTP-cache fallback crosses the boundary.

## Tradeoffs and remaining scope

Transfers intentionally copy bounded Blob bytes. This preserves explicit realm
and process ownership and independent revocation, but oversized or revoked
entries remain rejected or unavailable rather than becoming unbounded shared
state. Channel-message `MessageEvent.origin` intentionally remains the HTML
default empty string. Service Worker client-message Blob transfer, media
consumers, and the remaining browser/Web IDL certification gates remain
separate issue #40 slices.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked message_port -- --nocapture` (11 passed)
- `cargo test -p glass-browser --test native_engine --locked native_message_ports_transfer_blob_urls_between_page_and_worker_realms -- --nocapture` (1 passed)
- `cargo test -p glass-browser --test native_engine --locked native_content_process_message_ports_transfer_blob_urls_between_page_and_worker -- --nocapture` (1 passed)
- `cargo test -p glass-browser --test native_engine --locked window_proxy_message_port_transfers_blob_urls -- --nocapture` (2 passed)
- `cargo test -p glass-browser --test native_engine --locked native_content_process_service_worker_transfers_message_port_round_trip -- --nocapture` (1 passed)
- `git diff --check`
