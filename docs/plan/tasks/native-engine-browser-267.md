# Glass native engine browser slice 267: WritableStream and piping

Status: completed locally.

## Objective

Give native page code a bounded writable sink and a usable readable-to-writable
pipeline so stream producers can deliver values to application-owned sinks
with explicit ownership, close, cancellation, and error propagation.

## Contract

- `new WritableStream({ start, write, close, abort })` creates a bounded native
  sink and invokes sink operations in serialized order.
- `getWriter()` exposes one writer lock with `ready`, `desiredSize`, `closed`,
  `write()`, `close()`, `abort()`, and `releaseLock()`.
- Writer writes are finite and serialized; sink failure transitions the stream
  to an errored state and rejects lifecycle promises.
- `ReadableStream.pipeTo()` transfers values, closes the destination on normal
  completion unless prevented, cancels the source on pipeline failure unless
  prevented, and aborts the destination on source/sink failure unless
  prevented.
- `pipeThrough()` starts a bounded pipe into a `{ writable, readable }`
  transform surface and returns the readable side.
- Pipeline cleanup releases both reader and writer locks, including failure and
  abort paths.

## Implementation

The native bootstrap now owns WritableStream sink state, start/write/close/
abort sequencing, finite write accounting, writer closed waiters, and one
writer lock. Readable pipelines reuse the existing reader and cancellation
algorithms, while a bounded Promise race handles optional AbortSignal
termination. A sink write failure remains abortable long enough for `pipeTo`
to invoke the sink's `abort()` hook before releasing the writer.

## Tradeoffs and follow-up

The sink queue uses the existing native stream bound and does not implement
the full WritableStream controller, strategy, or Web IDL surfaces. There is no
TransformStream implementation yet; `pipeThrough()` accepts an externally
provided writable/readable pair. BYOB, transfer, upload progress, and full
Streams parity remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_streams_pipe_to_bounded_writable_sinks --locked -- --nocapture --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine stream --locked -- --test-threads=1 --nocapture` (11 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine response --locked -- --test-threads=1 --nocapture` (5 passed)
- `git diff --check`

The evidence is local-only. Remote CI, release, registry publication, and
final native/CDP parity claims remain pending the wider issue #40 gates.
