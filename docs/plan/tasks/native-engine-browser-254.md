# Glass native engine browser slice 254: incremental Fetch response bodies

Status: completed locally.

## Objective

Move page-visible Fetch response bodies from one buffered payload to a real
process-owned response stream after response headers are available. The
native content process must deliver bounded transport chunks through the
persistent JavaScript realm while retaining the existing security policy,
response metadata, and buffered behavior for non-script consumers.

## Contract

- Same-origin and CORS-authorized script Fetch responses expose a
  transport-backed `ReadableStream`. The content worker divides transport
  data into bounded 8 KiB chunks and enforces the existing per-response byte
  limit (16 MiB by default for the script path).
- `ReadableStream` readers receive each delivered chunk, a terminal `done`
  read, and typed stream errors through the same serialized QuickJS owner as
  Fetch promise continuations, timers, WebSocket events, EventSource events,
  and DOM mutations. A waiting script evaluation remains alive while a
  requested body read is pending.
- `Response.text()`, `json()`, `blob()`, `arrayBuffer()`, and `bytes()` remain
  bounded convenience operations over the stream owner's retained chunk
  history. `Response.clone()` creates a fresh response and stream view over
  that bounded history so existing clone/body consumers remain compatible.
- Opaque and opaque-redirect responses stay filtered and inaccessible. The
  shared loader continues to own URL resolution, redirect, CSP, mixed-content,
  CORS, referrer, cookie, timeout, and response-size policy. Parent navigation,
  downloads, and other non-script loader callers retain their buffered
  response contract.
- `Blob`, `File`, and `ReadableStream` constructor identities persist across
  bootstrap turns. A response body produced after an asynchronous transport
  event therefore remains an instance of the constructors visible to the
  page's persistent realm.

## Implementation

`NativeResourceLoader::open_fetch_response_stream_async` performs the normal
request and response-header policy, then transfers the live `reqwest` body to
the content worker. The existing buffered Fetch API drains that owner through
the new helper, so download and navigation callers do not receive a partial
body.

The content worker owns one bounded stream task per script response. Typed
read/cancel commands and chunk/end/error events cross the worker boundary;
the worker retains the terminal handshake long enough for a continuation's
last queued read to be processed. The event-loop path waits briefly when a
JavaScript reader or body convenience method has requested more data, then
dispatches the event and any follow-up read through the ordinary mutation
transaction.

The JavaScript bootstrap stores bounded stream groups in the persistent realm,
resolves pending readers and body-method waiters, and keeps the existing
reader lock/release/async-iterator behavior. Canonical Blob/File and
ReadableStream constructors avoid cross-bootstrap `instanceof` regressions
when a response settles after a network event.

## Tradeoffs and follow-up

This slice provides incremental delivery and bounded channel retention, but it
does not claim complete Fetch Streams/Web IDL conformance. The host currently
uses a bounded event queue rather than strict pull backpressure; reader
cancel does not yet prove end-to-end transport cancellation; `bodyUsed`,
`tee()`, BYOB readers, piping, response trailers, service-worker
interception, and full task-source fairness remain open. Convenience body
methods intentionally wait for the complete bounded response, and the
`Response` constructor still rejects page-created stream bodies. These are
explicit issue #40 follow-ups, not silent fallbacks.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises --locked -- --test-threads=1 --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_streams_fetch_response_body_incrementally --locked -- --test-threads=1 --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_drives_ --locked -- --test-threads=1 --nocapture` (4 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_fetches_ --locked -- --test-threads=1 --nocapture` (7 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_resolves_page_script_fetch_before_publish --locked -- --test-threads=1 --nocapture` (1 passed)
- `git diff --check`

Repository-wide strict Clippy was attempted but remains red on pre-existing
`-D warnings` debt across the native backend and unrelated workspace modules;
the changed code passed the locked compiler and focused behavioral gates.
Evidence is local-only. No push, remote CI, release, registry publication,
or native/CDP parity claim is made by this checkpoint.
