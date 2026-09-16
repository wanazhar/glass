# Native worker ReadableStream parity (392)

```yaml
id: native-engine-browser-392
scope: native-engine/worker-readable-stream-source-and-tee
status: done
depends-on:
  - native-engine-browser-391
```

## Objective

Close the worker-realm ReadableStream parity gap left by the transferable
stream slice. Worker-created streams must accept underlying sources instead of
being limited to byte snapshots, and a worker must be able to tee either a
local stream or a stream transferred from a page without flattening it.

## Contract

- `new ReadableStream(underlyingSource, strategy)` accepts default and
  `type: "bytes"` sources with bounded `highWaterMark` and `size` algorithms.
- Worker controllers expose bounded `desiredSize`, `enqueue`, `close`,
  `error`, and byte-stream `byobRequest` response methods. `start`, `pull`,
  and `cancel` are invoked with the correct source lifecycle and async error
  propagation.
- Default readers preserve arbitrary in-realm chunk values. Byte readers and
  BYOB readers preserve byte mode, partial buffers, source locking, disturbed
  state, cancellation, and `closed` settlement.
- `ReadableStream.prototype.tee()` owns one upstream reader, delivers each
  chunk to both bounded branches, propagates close/error, and cancels the
  upstream only after both branches cancel. The same owner works for streams
  received through the hidden cross-realm transfer endpoint.
- Snapshot-backed worker Request and Response constructors reject source-backed
  or remotely transferred streams explicitly until their asynchronous body
  transport is implemented; they must not silently serialize them as empty
  bodies.
- Existing command, message, stream, script, and realm limits remain
  authoritative. No CDP or eager whole-stream fallback is introduced.

## Delivered behavior

- Worker ReadableStream construction now distinguishes transfer descriptors,
  static byte snapshots, and user underlying sources, while retaining the
  constructor identity across bootstrap refreshes.
- Added bounded worker queue accounting, strategy validation, source
  controller methods, pending default/BYOB reads, source start/pull/cancel
  lifecycle, and rejecting `reader.closed` promises for source errors.
- Replaced the snapshot-only worker tee rejection with the shared demand-driven
  branch owner. A page-transferred byte stream can now be tee’d in the worker
  and read in order on both branches.
- Source-backed worker body use now fails with a typed `TypeError` at the
  synchronous snapshot boundary, making the remaining upload/response body
  transport gap observable rather than corrupting request or response data.
- Extended the local page-to-worker conformance fixture with worker-created
  underlying-source reads and transferred-stream tee coverage.

## Tradeoffs and explicit follow-up

The worker source owner uses bounded JavaScript queues and copies byte views at
controller/read boundaries, matching the existing page owner and preserving
realm ownership. The host scheduler remains operation-boundary driven, so
source promises and transferred pulls may require subsequent host turns. This
keeps the implementation deterministic and resource-bounded at the cost of
not providing an autonomous worker event loop. Streaming worker Request and
Response body transport, upload backpressure, worker `pipeTo`/`pipeThrough`
and TransformStream parity, cross-source task arbitration, broader
transferables, and final production certification remain issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-392.md`

## Verification

Validation completed locally against this checkpoint:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_readable_stream_transfer_preserves_pull_order_and_hides_transport_port --locked` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_drives_underlying_readable_stream_sources --locked` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_resolves_readable_stream_reader_closed --locked` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_tees_underlying_readable_stream_sources --locked` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_cancels_tee_upstream_after_both_branches_cancel --locked` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_tees_fetch_streams_until_all_readers_cancel --locked` — 1 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
