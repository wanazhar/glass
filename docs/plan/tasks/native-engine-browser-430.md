# Native content-process worker XHR streaming (430)

status: complete
scope: native-engine/content-process-worker-xhr-streaming
issue: 40
depends-on: [native-engine-browser-429]

## Objective

Close the content-process worker-XHR response-streaming gate by giving worker
Fetch responses the same bounded, demand-driven response transport already
used by page Fetch and page XHR. Worker XMLHttpRequest must expose truthful
response lifecycle and progress observations as network chunks are admitted,
including response text split across UTF-8 boundaries.

## Contract

- The content-process owner shares one bounded Fetch response-stream transport
  between page and worker realms.
- Worker-owned `FetchStreamRead` and `FetchStreamCancel` commands carry and
  validate their worker owner before they reach the transport.
- Worker XHR publishes `HEADERS_RECEIVED`, one `LOADING`/`ProgressEvent` pair
  for each admitted non-empty response chunk, and one terminal `DONE`/`load`/
  `loadend` sequence.
- Text and empty response types preserve UTF-8 text across chunk boundaries;
  binary, JSON, and Blob response projections continue to use the bounded
  final-byte decoders.
- Abort, reopen, stream cancellation, transport errors, and worker teardown
  cannot let stale response continuations mutate the reused XHR.
- Cached and opaque/no-body responses remain valid through the buffered path.

## Tradeoffs

- Reusing the page stream transport keeps response limits, cancellation, and
  backpressure in one owner, but worker network progress now requires a
  content-process event-pump turn.
- The worker realm retains bounded chunks for response clones and readers so
  clones created before completion remain independently readable; the existing
  worker response-body limit remains authoritative.
- The in-process `NativeEngine` worker registry stays buffered because that
  owner has no asynchronous worker-stream pump. This is an explicit ownership
  boundary, not a claim of all-owner streaming parity.
- Upload streaming, synchronous XHR, streaming XML, and complete XHR/Streams
  Web IDL parity remain separate issue #40 gates.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker_xhr_streams_response_progress --locked -- --nocapture`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker_xhr --locked -- --nocapture` (2 passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker_exposes_xhr_fetch_bridge --locked -- --nocapture`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker_fetch --locked -- --nocapture` (3 passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_ --locked -- --nocapture` (11 passed)
- `git diff --check`

The new content-process witness passed with incremental body chunks,
split-UTF-8 decoding, monotonic byte progress, and the expected XHR states.
The worker-fetch compatibility, binary, clone, page-XHR, upload, and abort
regressions also passed. Remote CI, push, release, tag, and registry
publication are outside this local checkpoint.
