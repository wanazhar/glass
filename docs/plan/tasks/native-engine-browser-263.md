# Glass native engine browser slice 263: ReadableStream reader lifecycle

Status: completed locally.

## Objective

Make terminal state of page-created native `ReadableStream` instances
observable through the standard default-reader lifecycle surface. A reader
must not wait forever when its stream closes, errors, or is canceled after the
reader has released its lock.

## Contract

- `reader.closed` resolves when a source closes or is canceled.
- `reader.closed` rejects when a source errors, using the same bounded error
  surface as a rejected `read()`.
- A reader released before terminal state remains attached to the stream's
  terminal notification, while a later reader observes an already-terminal
  stream immediately.
- Reader cancellation and `ReadableStream.prototype.cancel()` use the same
  stream terminal notification path.
- Fetch-backed streams retain their existing transport demand, clone, and
  cancellation behavior.

## Implementation

Native stream state now owns a bounded list of reader-closed waiters. Source
close, source error, source cancellation, Fetch transport completion, and
Fetch transport failure settle that list exactly once. Reader creation either
registers its promise or resolves/rejects immediately from existing terminal
state; lock release no longer drops lifecycle observation.

## Tradeoffs and follow-up

This closes the lifecycle hole without expanding the implementation into the
full Streams standard. BYOB readers, tee algorithms for page-created streams,
piping, transfer strategies, queuing strategies, and complete Streams/Web IDL
parity remain issue #40 work. The waiter list is bounded by the engine's
single-lock stream model and is drained on terminal transition.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_resolves_readable_stream_reader_closed --locked -- --nocapture --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine readable_stream --locked -- --test-threads=1 --nocapture` (2 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine request_body --locked -- --test-threads=1 --nocapture` (2 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine fetch --locked -- --test-threads=1 --nocapture` (18 passed)
- `git diff --check`

The evidence is local-only. Remote CI, release, registry publication, and
final native/CDP parity claims remain pending the wider issue #40 gates.
