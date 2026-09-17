# Native engine browser-complete slice 464: cross-realm Blob message transfer

- Status: complete locally
- Scope: `native-engine` / structured page-worker Blob URL ownership
- Issue: #40
- Depends on: [native-engine-browser-463](native-engine-browser-463.md)

## Objective

Make a Blob URL referenced by a structured message usable in the destination
dedicated or shared worker realm. Realm-local URL registries must remain
isolated, while the browser-owned message boundary carries a bounded resource
snapshot through both the inline and content-process owners.

## Contract

- A page-owned Blob URL embedded at any bounded depth in a worker message is
  snapshotted before the message leaves the page realm and is available to
  worker Fetch/XHR after delivery.
- A worker-owned Blob URL embedded in a worker-to-page message is snapshotted
  before the message leaves the worker realm and is available to page
  Fetch/XHR after delivery.
- Dedicated and shared worker message paths use the same typed transfer
  envelope; legacy messages without transfers remain compatible.
- The destination installs each transfer before structured-clone decoding,
  preserving the original URL string while keeping source-realm revocation
  independent from the destination snapshot.
- Transfer count, URL, MIME, byte, nesting, aggregate-message, and
  content-process framing limits remain enforced; malformed transfers fail
  closed without network or cache fallback.
- Inline fixture and HTTP(S) content-process owners share this contract;
  ordinary worker creation, MessagePort transfers, network Fetch, and
  existing Blob behavior remain unchanged.

## Implementation

- Added a bounded `NativeObjectUrlTransfer` envelope and structured-message
  scanner for active Blob URLs.
- Enriched page and worker `WorkerPostMessage` commands with resource
  snapshots, validated the payload at worker and page event queues, and
  carried the envelope through content-process serialization.
- Added destination registry installation for page and worker bootstrap
  realms before message decoding.
- Reused the existing Blob byte limits and object URL registry owners; no
  network/cache lookup or shared live JavaScript object crosses a realm.

## Tradeoffs and remaining scope

Transfers intentionally copy bounded Blob bytes, so large or revoked entries
remain rejected or unavailable rather than becoming unbounded shared state.
This slice covers page/dedicated/shared-worker message delivery; page-window,
MessagePort, Service Worker client messaging, media consumers, and the
remaining Web IDL/browser conformance gates remain separate issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked native_local_worker_blob_object_urls_cross_realm_messages -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_local_worker_ -- --nocapture`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_worker_blob_object_urls_cross_realm_messages -- --nocapture`
- `git diff --check`
