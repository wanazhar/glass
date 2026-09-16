# Native engine browser-complete slice 434: Service Worker Fetch uploads

status: complete
scope: native-engine/service-worker-fetch-request-upload-streaming
issue: 40
depends-on: [native-engine-browser-433]

## Objective

Close the Service Worker-owned Fetch request-body streaming gate. A
`ReadableStream` used by a Service Worker-created `Request` must remain
one-shot and demand-driven at the HTTP boundary instead of being rejected or
drained into a whole-body buffer.

## Contract

- A Service Worker can pass a stream-backed `Request` or Fetch options to
  `fetch()` and retain normal Request body ownership rules.
- The content process creates one bounded upload connection per active
  Service Worker Fetch. HTTP demand re-enters the owning Service Worker realm
  for exactly one chunk, end, error, or cancellation signal.
- Worker-side and content-process boundaries enforce the existing body-byte
  limit and bounded upload chunk-count limit.
- Reader ownership is released on completion and cancelled on stream error,
  request failure, redirect replay rejection, or worker teardown.
- A body-bearing 301/302/303 response may switch to a bodyless GET. A 307/308
  response, or any other replay requiring the one-shot body, rejects with a
  network error.
- Existing buffered Service Worker fetches and controlled POST clone/replay
  remain available. Page-originated controlled streaming interception remains
  a separate bridge because it has two realm-owned producers.

## Implementation

- `javascript.rs` exposes the Service Worker upload demand dispatcher through
  the existing worker event bridge and accepts upload commands for the
  Service Worker owner.
- `service_worker.rs` owns request-scoped upload connections, validates
  worker/request identifiers and demand order, drives upload demand through
  the Service Worker realm, and returns unrelated callback commands to the
  existing bounded command queue.
- `resource_loader.rs` adds a shared full-response collector for an already
  opened request-body stream, preserving response limits and network shaping
  while reusing the existing redirect and policy machinery.
- Service Worker teardown and registration replacement drop/cancel the
  request-body route; buffered request handling remains unchanged.

## Tradeoffs

- The Service Worker event loop now waits for stream-backed upstream Fetch
  demand. This reduces buffering and preserves backpressure, but a slow
  `pull()` callback delays the upstream network request by design.
- The Service Worker upload path buffers the upstream response before
  resolving the worker Fetch promise, matching the existing Service Worker
  response envelope. This slice does not claim streaming `Response` delivery
  through `respondWith`.
- Request-body streams are explicitly non-replayable. This avoids implicit
  cloning or memory amplification and rejects body-preserving redirects.
- Page-originated controlled stream interception is not folded into this
  slice: routing a page reader through a Service Worker and potentially back
  to HTTP needs an explicit two-realm ownership contract.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_service_worker_replays_cloned_request_body --locked -- --nocapture`
- `git diff --check`

The real HTTP witness retains the existing controlled POST clone/replay
assertions and adds a Service Worker-created two-chunk stream-backed POST.
The server observes `Transfer-Encoding: chunked` and exact
`native-sw-stream` bytes, while the buffered replay path still returns its
original JSON result.
