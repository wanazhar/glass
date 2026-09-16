# Native engine browser-complete slice 433: Worker Fetch request uploads

status: complete
scope: native-engine/worker-fetch-request-upload-streaming
issue: 40
depends-on: [native-engine-browser-432]

## Objective

Close the content-process Worker and SharedWorker Fetch request-body
streaming gate. A worker-owned `ReadableStream` body must remain one-shot and
demand-driven at the HTTP boundary instead of being drained into a bounded
JavaScript buffer before dispatch.

## Contract

- Dedicated and SharedWorker Fetch accept a `ReadableStream` body from a
  `Request` or Fetch options and assign a request-scoped upload identifier.
- The content process creates one bounded HTTP request body stream per active
  worker request. Each HTTP demand admits exactly one worker chunk, end,
  error, or cancellation signal.
- Worker-side and content-process boundaries enforce the existing body-byte
  limit and the bounded upload chunk-count limit.
- Worker reader ownership is preserved: a stream is locked and disturbed for
  the request, released on completion, and cancelled on abort, error, worker
  teardown, or native request completion.
- A body-bearing 301/302/303 response may switch to a bodyless GET. A 307/308
  response, or any other replay requiring the one-shot body, rejects with a
  network error rather than buffering or silently replaying it.
- The existing buffered worker Fetch path remains unchanged for non-stream
  bodies. Controlled Service Worker upload replay remains a separate bridge.

## Implementation

- `javascript.rs` owns worker upload groups, reader locking/release, bounded
  byte accounting, and the demand callback dispatched into dedicated and
  SharedWorker realms.
- `NativeWorkerRegistry` owns `(worker_id, request_id)` upload connections,
  validates command ownership and demand order, and cancels routes when a
  worker is removed or the registry is cleared.
- The content-process worker resolver uses a task-local loader snapshot while
  driving upload demand, then merges cookie/cache/CSP state back into the
  live loader after the HTTP task completes.
- The shared native resource-loader body transport supplies backpressure and
  rejects redirect replay of the one-shot stream according to the page upload
  contract.
- Controlled Service Worker upload replay is intentionally not widened by
  this slice; it retains its separate request-body ownership boundary.

## Tradeoffs

- Worker uploads now wait for worker `pull()` work at the HTTP boundary. This
  reduces peak buffering but makes a slow worker callback visible as network
  backpressure.
- A worker may still use the buffered path for byte/text/blob bodies, keeping
  the existing fast path and response behavior stable while stream uploads
  pay the extra demand/event-turn cost.
- The upload connection is request-scoped and non-replayable. This preserves
  ownership and bounded memory but rejects redirects that would require a
  second read of the worker stream.
- This is a bounded Fetch/Streams contract, not complete Web IDL parity;
  synchronous XHR, controlled Service Worker streaming replay, fixture-owned
  bodyful requests, and remaining XHR/Streams gates stay issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker_fetch_preserves_binary_request_and_response_bodies --locked -- --nocapture`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_fetches_stream_request_bodies_and_clones_them --locked -- --nocapture`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_commits_page_work_from_stream_upload_pull --locked -- --nocapture`
- `git diff --check`

The worker HTTP witness observes chunked transfer encoding and the exact
`worker-stream` body for both a stream-backed `Request` and stream-backed
Fetch options. The existing binary Request/response assertions remain in the
same test, proving the buffered worker path stays available. The content
owner keeps the body bounded and one-shot; controlled Service Worker upload
replay is not represented as complete by this slice.
