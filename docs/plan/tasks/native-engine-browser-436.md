# Native engine browser-complete slice 436: controlled page stream uploads

status: complete
scope: native-engine/controlled-page-fetch-upload-streaming
issue: 40
depends-on: [native-engine-browser-435]

## Objective

Close the page-originated controlled Service Worker Fetch upload gate. A page
`ReadableStream` request body must reach the active Service Worker's
`FetchEvent` with its bytes and request metadata intact instead of being
rejected merely because the page and worker are separate JavaScript realms.

## Contract

- A controlled page Fetch may use a bounded `ReadableStream` request body.
- The page realm remains the producer: its reader is advanced only by the
  existing upload-demand event path, and chunk, end, error, cancellation, and
  byte/chunk limits retain their existing semantics.
- The content process materializes the admitted one-shot chunks into one
  bounded replayable body before invoking the Service Worker FetchEvent. The
  worker therefore receives the same normal `Request` body projection used by
  buffered controlled requests.
- The original document URL and request metadata are captured when Fetch is
  scheduled, so mutations performed by stream `pull()` callbacks cannot
  retarget the request.
- A handled worker response resolves through the normal page Fetch path. An
  unhandled request continues through the native HTTP loader with the same
  materialized body; stream failures reject the page Fetch without crashing
  the content process.
- Existing direct HTTP(S) page and Worker upload paths remain demand-driven at
  the transport boundary. This slice does not claim zero-copy streaming from
  a page reader through a Service Worker and back to HTTP.

## Implementation

- `fetch_stream.rs` factors the upload command channels into a reusable source
  that can become either a `reqwest::Body` or a bounded collector.
- `content_process.rs` adds a separate controlled-upload task map, preserves
  request metadata, collects page chunks under the existing limits, and feeds
  the completed body into `NativeServiceWorkerRegistry::intercept_fetch`.
- The existing FetchUpload task source continues to dispatch page stream
  pulls, page mutations, cancellation, and terminal commands; no second page
  stream producer is introduced.
- The Service Worker replay test now verifies an intercepted page stream and
  asserts that the intercepted URL produces no extra server request.

## Tradeoffs

- Materialization adds one bounded copy and delays the worker FetchEvent until
  the page body ends. This is necessary with the current two-realm event
  protocol and gives the worker a replayable request body with deterministic
  ownership.
- Direct network uploads retain true demand-driven backpressure; controlled
  page uploads pay the bounded handoff cost but gain Service Worker
  interception instead of a false unsupported error.
- If the worker does not handle the request, the body is reused for the normal
  native HTTP request. Body-preserving redirects remain subject to the same
  one-shot replay rules as other streamed uploads.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_service_worker_replays_cloned_request_body --locked -- --nocapture` (1 passed)
- `git diff --check`

The HTTP Service Worker witness returns the exact two-chunk `page-stream`
body from an intercepted controlled POST. The server still observes only the
buffered replay and Service Worker-created upstream stream requests, proving
the page stream was handled inside the worker rather than silently bypassing
the Service Worker.
