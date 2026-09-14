# Native cross-realm MessagePort transfer (337)

Status: implemented locally in the current 0.3.14 checkout.

This slice closes the cross-realm transfer boundary for page-created and
dedicated-worker `MessagePort` endpoints in both native execution paths:

- `postMessage(message, transfer)` accepts bounded transfer lists in page and
  dedicated-worker realms, clones the message payload, and exposes transferred
  endpoints through `MessageEvent.ports`;
- transferring an endpoint detaches the source endpoint, preserves the
  entangled peer, and rejects reuse or invalid transfer members with typed
  `DataCloneError`/`InvalidStateError` behavior;
- the native owner assigns a stable, realm-qualified bridge key to each
  transfer, validates it at the Rust boundary, and routes messages by that key
  across both the in-process fixture engine and the HTTP(S) content process;
- page-to-worker and worker-to-page delivery both support worker-created ports,
  including replies through the returned endpoint; and
- transfer queues, payloads, bridge identifiers, and nested transfer lists
  remain bounded. No JavaScript object, pointer, or unvalidated realm state
  crosses the host boundary.

## Tradeoffs

- Bridge-key delivery is used instead of a page-local numeric endpoint ID. A
  worker-created port receives a new page-local object when it arrives, so the
  stable key is the only identity that remains valid on both sides. This also
  keeps the in-process and content-process owners on one routing contract.
- MessagePort delivery is retained as a separate bounded queue from ordinary
  `Worker` events. Each queue preserves its own enqueue order; the host drains
  them at the next page turn rather than pretending that two independent task
  sources have browser-accurate interleaving.
- The implementation uses JSON-backed structured-clone coverage already used
  by the native runtime. Transferable ports are supported; richer browser
  transferables and SharedWorker/service-worker channel ownership remain
  separate profile work.

## Local evidence

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_message_channels_deliver_events_in_page_and_worker_realms --locked` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_message_ports_transfer_between_page_and_worker_realms --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_transfers_message_ports_between_page_and_worker_realms --locked -- --nocapture` — 1 passed

The two integration witnesses cover local and content-process page-to-worker
transfers, worker-created ports returned to the page, detached-source errors,
asynchronous `MessageEvent.ports`, and replies in both directions. All evidence
is local; this checkout has not been pushed and has no remote CI result.
