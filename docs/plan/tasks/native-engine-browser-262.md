# Glass native engine browser slice 262: ReadableStream sources

Status: completed locally.

## Objective

Make page-created `ReadableStream` instances useful instead of silently
collapsing an underlying source into an empty stream. Provide bounded source
lifecycle, demand, cancellation, and controller ownership while preserving
the existing Fetch transport stream path.

## Contract

- `new ReadableStream({ start, pull, cancel })` is accepted with a bounded
  underlying source. `start` runs once, `pull` runs on demand, and `cancel`
  receives the caller's reason.
- The controller exposes bounded `desiredSize`, `enqueue`, `close`, and
  `error` operations. Queued chunks are capped by the existing stream bound;
  oversized chunks and queue overflow reject explicitly.
- Default readers receive source-enqueued values, preserve lock/release and
  async-iteration behavior, and resolve pending reads on enqueue, close,
  error, or cancellation.
- Source cancellation marks the stream disturbed and invokes the source
  cancellation hook. Fetch-backed streams retain their existing byte,
  transport-demand, clone, and content-process ownership behavior.
- Byte readers/BYOB mode, piping/transfer strategies, full queuing-strategy
  semantics, and complete Streams/Web IDL parity remain open.

## Implementation

The native stream state now distinguishes byte-backed Fetch/static streams from
page-created underlying-source streams. It owns the source controller, finite
queue, start/pull promise state, disturbance callback, and source cancellation
path. Generic source chunks pass through the default reader unchanged, while
the existing byte mode still returns `Uint8Array` chunks and requests Fetch
transport demand as before.

The integration witness enqueues one value during `start`, supplies the next
value from demand-driven `pull`, closes the stream, verifies lock/release, and
checks that `cancel(reason)` reaches the source callback.

## Tradeoffs and follow-up

The source model is deliberately bounded and default-reader-only. It enables
common page-created streams without pretending to implement BYOB memory
ownership, transfer/piping, high-water-mark strategies, or every Streams
algorithm. A later slice can use this source owner to implement streaming
Request uploads once asynchronous body handoff and cancellation are wired.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_drives_underlying_readable_stream_sources --locked -- --nocapture --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine request_body --locked -- --test-threads=1 --nocapture` (2 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine fetch --locked -- --test-threads=1 --nocapture` (18 passed)
- `git diff --check`

The evidence is local-only. Remote CI, push, release, registry publication,
and final native/CDP parity claims remain pending the wider issue #40 gates.
