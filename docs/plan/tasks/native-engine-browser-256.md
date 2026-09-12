# Glass native engine browser slice 256: Fetch stream demand and cancellation

Status: completed locally.

## Objective

Close the native Fetch response-stream ownership gap left by slice 254. A
page-readable response body must not cause the content worker to drain the
entire HTTP response before JavaScript asks for more data, and cancellation of
the page stream must reach the live transport owner.

## Contract

- The content worker waits for an explicit reader/body demand before reading
  the next transport chunk and emits at most one bounded 8 KiB part for that
  demand. Additional parts from the same received transport chunk stay in a
  bounded worker queue until later demand arrives.
- A reader `read()` with no queued part creates one pending demand. A queued
  part satisfies the current read and the next read creates the next demand.
  Full-body convenience methods create a body demand and continue requesting
  parts until the terminal event.
- `ReadableStream` reader `cancel()`/`return()` and unlocked stream
  `cancel()` clear the page-side queue and send one typed
  `FetchStreamCancel` command when no clone or full-body consumer still owns
  the stream group. The content process removes the stream connection and
  drops the live `reqwest` response body, allowing the peer to observe close.
- End and error events remain terminal. Cancellation resolves pending reader
  reads as `{ done: true }`; it does not turn an intentional cancellation into
  a synthetic network error.
- Opaque and opaque-redirect responses retain their filtered `body === null`
  behavior. The existing response-size, chunk-size, command-count, and event-
  loop bounds remain authoritative.
- Body disturbance/`bodyUsed`, locked-or-consumed clone rejection, shared tee
  semantics, BYOB readers, piping, trailers, and complete Fetch Streams/Web
  IDL behavior remain separate promotion work.

## Implementation

`run_native_fetch_stream` now has an explicit pull state and retains only the
remaining parts of the one transport chunk currently being served. The page
realm tracks one outstanding demand per stream group, requests more data only
for a pending reader or full-body waiter, and propagates group cancellation
through the existing typed command wire. Clone consumers keep a transport
alive until their sibling stream states are also canceled.

The existing binary Fetch compatibility witness now uses a clone for later
body reads after canceling the original reader, matching the ownership
boundary rather than relying on independent reads from a canceled stream. A
new HTTP integration witness leaves the response body open, cancels the page
reader after the first part, and verifies that the server observes the client
connection close.

## Tradeoffs and follow-up

This is bounded pull control, not a claim of the browser Streams standard's
complete queuing-strategy or high-water-mark model. A received transport chunk
may still contain several 8 KiB parts, and the JavaScript owner intentionally
retains bounded history so existing convenience body methods and bounded
clones can read it. The next Fetch work should add body disturbance and
`bodyUsed`, correct clone/tee ownership, and then BYOB/piping/trailer/Web IDL
parity. XHR upload cancellation and unrelated request-ledger granularity
remain separate contracts.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_ --locked -- --test-threads=1 --nocapture` (101 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises --locked -- --exact --test-threads=1 --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_cancels_fetch_response_stream_transport --locked -- --exact --test-threads=1 --nocapture` (1 passed)
- `git diff --check`

The evidence is local-only. Remote CI, push, release, registry publication,
and final native/CDP parity claims remain pending the wider issue #40 gates.
