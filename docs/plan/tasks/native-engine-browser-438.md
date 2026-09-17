# Native engine browser-complete slice 438: synchronous XHR

status: complete
scope: native-engine/synchronous-xhr
issue: 40
depends-on: [native-engine-browser-437]

## Objective

Close the synchronous XMLHttpRequest gate for the native page and worker
realms without creating a second request-policy implementation. A bounded
`open(..., false)` request must use the existing native loader for fixture and
HTTP(S) owners, publish the completed response through the existing XHR
projections, and preserve loader side effects for the owning browser process.

## Contract

- Page and dedicated/SharedWorker XMLHttpRequest instances accept
  `async === false` and block only the current script turn until the bounded
  request completes.
- GET/HEAD body restrictions, request-header validation, credentials, CORS,
  redirects, cookies, cache state, CSP observations, and response-size limits
  remain owned by `NativeResourceLoader`.
- The existing text, JSON, ArrayBuffer, Blob, XML, and HTML response
  projections are available after completion; the synchronous lifecycle emits
  `OPENED` followed by the terminal `DONE` turn with the existing upload and
  load/error/loadend behavior.
- Non-zero synchronous XHR timeouts and ReadableStream request bodies fail
  closed with the bounded native error contract. Synchronous XHR does not
  claim incremental response progress or complete XHR/Web IDL parity.

## Implementation

- `NativeJavaScriptRuntime` owns a bounded loader snapshot slot for one host
  call. The page and worker bootstrap payloads use the same request-body
  normalization and response projection paths as asynchronous XHR.
- The host callback runs the existing async loader on a short-lived
  current-thread Tokio runtime inside a dedicated OS thread, because QuickJS
  cannot await from a synchronous callback. The updated loader snapshot is
  merged back into the live owner for cache, cookie, and CSP state.
- Local `NativeEngine` evaluation, navigation, lifecycle, dynamic-script, and
  worker event paths refresh and merge the slot explicitly. The content
  process refreshes it at command boundaries and persists the merged state
  with the normal runtime-state synchronization path.
- Worker stream, upload, WebSocket, EventSource, MessagePort, and Fetch event
  dispatches use the same loader-aware worker evaluation wrapper, so a sync
  XHR from any worker callback cannot lose its loader effects.
- Already-buffered cache/revalidation responses stay on the bounded buffered-
  response path and are exposed as local JavaScript `ReadableStream` objects.
  Only live network bodies create demand-driven stream connections, avoiding
  repeated full-bootstrap turns for cache hits.

## Tradeoffs

- The synchronous call necessarily blocks the JavaScript turn and uses one
  short-lived host thread/runtime per request. This preserves synchronous
  script semantics while avoiding a deadlock in the async executor, at the
  cost of setup overhead that is inappropriate for high-volume requests.
- The response is fully buffered before the XHR reaches `DONE`; no fabricated
  `LOADING` or socket-level progress is reported. Incremental XHR response
  streaming remains owned by the asynchronous path. This is separate from
  Fetch response streams: cache hits are already buffered while live network
  responses remain demand-driven.
- Loader snapshots are merged at the owner boundary rather than sharing
  mutable state across realms. This keeps the existing policy owner intact,
  but means synchronous XHR remains bounded by the same explicit native
  limits and does not provide browser-grade concurrent mutation semantics.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --nocapture` (20 passed, 0 failed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_page_and_worker_support_sync_xhr --locked -- --nocapture` (1 passed, 0 failed)
- `git diff --check`

The local fixture witness verifies a synchronous GET and terminal state. The
HTTP witness verifies page and Worker POST bodies, content types, response
headers, response URLs, status projections, and the bounded `[OPENED, DONE]`
ready-state sequence, including a later content-process evaluation turn. The
same XHR group also covers the cache/revalidation regression: buffered cache
responses no longer enter the live stream event path and the full group stays
green.
