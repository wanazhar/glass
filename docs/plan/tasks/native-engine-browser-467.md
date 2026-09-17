# Native engine browser-complete slice 467: Service Worker client Blob transfer

- Status: complete locally
- Scope: `native-engine` / Service Worker `Client.postMessage()` Blob URL ownership
- Issue: #40
- Depends on: [native-engine-browser-466](native-engine-browser-466.md)

## Objective

Make a Blob URL created by a Service Worker and sent with
`ServiceWorkerClient.postMessage()` usable in the destination page realm. The
worker, optional content-process, and page owners must carry one bounded typed
resource envelope and install it before the page Service Worker `message` event
decodes its structured clone.

## Contract

- A Service Worker-created Blob URL embedded at supported bounded depth in a
  client message is snapshotted in the worker realm before delivery.
- Worker, content-process, and page event boundaries validate the transfer
  count, URL, MIME, byte, nesting, aggregate-message, and framing limits.
- The destination installs the snapshot before structured-clone decoding; the
  original URL string is preserved and source-worker revocation remains
  independent.
- HTTP(S) client delivery uses the native Blob owner directly and never falls
  back to a network request or HTTP cache lookup.
- Messages without Blob URLs and existing MessagePort transfers remain
  compatible through serde defaults.

## Implementation

- Added a bounded `object_urls` envelope to Service Worker client commands and
  page events, with backwards-compatible serde defaults.
- Reused the active worker-context Blob URL scanner so `Client.postMessage()`
  snapshots resources at the source owner rather than trusting serialized
  caller-supplied transfer metadata.
- Validated and propagated the envelope through Service Worker queues,
  content-process response decoding, page event validation, and the page
  dispatcher.
- Installed destination snapshots before `glassMessageDecodeEnvelope()` runs,
  matching the MessagePort and Window message ownership rules.

## Tradeoffs and remaining scope

Transfers copy bounded Blob bytes. This gives the page an explicit resource
owner and independent revocation, but it intentionally rejects oversized or
revoked entries instead of creating unbounded shared state. MessagePort
`event.origin` metadata, media consumers, and the remaining browser/Web IDL
certification gates remain separate issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked` (passed; the
  test target compiled in 37.9s)
- `cargo test -p glass-browser --test native_engine --locked native_content_process_registers_service_worker_and_intercepts_fetch_and_navigation -- --nocapture` (1 passed)
- The regression delivers a Service Worker-created Blob URL inside
  `Client.postMessage()`, then fetches it from the page and verifies body and
  MIME without a network route.
- `git diff --check`
