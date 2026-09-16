# Native engine browser-complete slice 432: page Fetch request uploads

status: complete
scope: native-engine/page-fetch-request-upload-streaming
issue: 40
depends-on: [native-engine-browser-431]

## Objective

Close the page Fetch request-body streaming gate for HTTP(S) documents. A page
`ReadableStream` used as a Fetch request body must remain a real one-shot,
demand-driven body at the native HTTP boundary instead of being drained into a
whole-response-sized JavaScript buffer first.

## Contract

- Page Fetch accepts a `ReadableStream` body from a `Request` or Fetch options
  and assigns it a request-scoped upload stream identifier.
- The content process creates one bounded HTTP request body stream per active
  request. The HTTP client requests the next body part before the page realm
  is asked for it; each demand admits exactly one chunk, end, error, or cancel.
- Body bytes are checked at both the page boundary and the content-process
  boundary against the existing form-body limit. A separate bounded chunk
  count prevents an unbounded sequence of empty or tiny chunks.
- Upload cancellation, stream errors, request completion, and navigation
  teardown release the page reader and terminate the host body stream.
- A body-bearing 301/302/303 response may switch the follow-up request to a
  bodyless GET. A 307/308 response, or any other replay that would require
  resending the one-shot stream, rejects with a network error rather than
  silently replaying or buffering it.
- A page upload `pull()` callback may emit ordinary page work. DOM, history,
  and newly scheduled page networking commands are committed through the same
  serialized page event path as other native host events.
- Worker upload streaming and controlled Service Worker upload replay remain
  explicit follow-up gates; Worker Fetch continues to use its existing
  bounded buffered request-body bridge.

## Implementation

- `fetch_stream.rs` owns the bounded async upload command/event transport and
  the upload chunk-count budget.
- `javascript.rs` tracks request-scoped upload groups, acquires and releases
  page readers, converts admitted chunks to bounded base64 commands, and
  propagates cancellation and errors.
- `content_process.rs` schedules upload demand as its own rotating task
  source, validates identifiers/order/size, starts the reqwest body stream,
  merges task-local loader state after completion, and processes page work
  emitted during upload demand.
- `resource_loader.rs` attaches the one-shot body only to the initial HTTP
  request and refuses replay across redirects that preserve a body.
- `service_worker.rs` rejects streaming request bodies at the controlled
  Service Worker bridge until that bridge can preserve the same ownership and
  replay contract.

## Tradeoffs

- The body is truly backpressured at the reqwest stream boundary, reducing
  peak memory for large uploads, but a slow page `pull()` callback can now
  delay the network request by design.
- A cloned page Request still uses the existing bounded `ReadableStream.tee`
  ownership model. Each Fetch receives an independent one-shot upload group;
  no stream is replayed implicitly.
- The content process uses a task-local loader snapshot so the page loop can
  continue servicing upload demand without borrowing the live loader across
  an await. Cookie/cache/CSP state is merged after the task finishes; this
  keeps ownership serialized but does not claim autonomous cross-process
  arbitration.
- Fixture-owned bodyful requests remain fail-closed because the fixture
  loader is deterministic and not a second streaming network stack.
- The chunk-count cap protects the event loop from pathological tiny chunks,
  while the existing byte cap remains the authoritative payload limit. This
  is bounded Web Fetch behavior, not a claim of complete Streams or Fetch Web
  IDL parity.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_fetches_stream_request_bodies_and_clones_them --locked -- --nocapture` (three consecutive runs passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_commits_page_work_from_stream_upload_pull --locked -- --nocapture` (passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_does_not_replay_stream_request_bodies_across_redirects --locked -- --nocapture` (passed)
- `git diff --check`

The HTTP witness observes decoded chunked request bodies, clone-independent
uploads, ordinary page work from `pull()`, a 302 method-switch success, and a
307 replay rejection. The content-process loader keeps the transport boundary
bounded and one-shot; worker and controlled Service Worker upload bridges are
not represented as complete by this slice.
