# Glass native engine browser slice 268: TransformStream

Status: completed locally.

## Objective

Provide a connected native `TransformStream` primitive so page code can use
the standard `readable`/`writable` pair with `ReadableStream.pipeThrough()`.

## Contract

- `new TransformStream({ start, transform, flush })` exposes native readable
  and writable sides.
- Transformer `start` runs once before writes; `transform` runs in serialized
  write order and can enqueue output; `flush` runs before the readable side
  closes.
- The transform controller exposes bounded `desiredSize`, `enqueue`, `error`,
  and `terminate` operations.
- Transformer output flows through the existing bounded readable queue and
  writer pipeline; output backpressure cannot grow without limit.
- Transformer errors reject the writable operation and error the readable
  side; normal source completion flushes and closes the transform output.

## Implementation

The native bootstrap creates a readable source controller and a writable sink
around one transform state. Writable `start`, `write`, `close`, and `abort`
operations invoke the corresponding transformer hooks, while the controller
routes output into the existing readable stream owner. `pipeThrough()` now
works with the standard TransformStream shape and still reuses the existing
lock, cancellation, and bounded queue paths.

## Tradeoffs and follow-up

This slice implements the common default-reader/default-writer transform
contract. It does not yet implement custom queuing strategies, byte/BYOB
controllers, transfer, full cancellation reason choreography, or every
TransformStream/Web IDL descriptor. These remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_transform_streams_process_pipe_through_values --locked -- --nocapture --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine stream --locked -- --test-threads=1 --nocapture` (12 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine response --locked -- --test-threads=1 --nocapture` (5 passed)
- `git diff --check`

The evidence is local-only. Remote CI, release, registry publication, and
final native/CDP parity claims remain pending the wider issue #40 gates.
