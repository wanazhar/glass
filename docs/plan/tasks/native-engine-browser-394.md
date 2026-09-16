# Native worker stream composition (394)

```yaml
id: native-engine-browser-394
scope: native-engine/worker-stream-composition
status: done
depends-on:
  - native-engine-browser-393
```

## Objective

Close the worker-realm stream-composition gap. A worker must expose the
bounded native `WritableStream` and `TransformStream` surfaces and be able to
compose worker `ReadableStream` instances with `pipeTo()` and `pipeThrough()`.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`

## Contract

- Worker `WritableStream` accepts an underlying sink, serializes writes and
  close through a bounded queue, exposes writer locking, `ready`, `desiredSize`,
  `closed`, and release behavior, and propagates start/write/close/abort
  failures.
- Worker `ReadableStream.prototype.pipeTo()` validates source, destination,
  options, and abort signals; preserves source and destination lock ownership;
  closes, aborts, and cancels according to the prevent flags; and releases
  both locks on every terminal path.
- Worker `ReadableStream.prototype.pipeThrough()` validates a native
  transform pair, starts the bounded pipe, and returns the transform's
  readable side without introducing a host/CDP boundary.
- Worker `TransformStream` exposes native readable and writable sides;
  transformer `start`, `transform`, `flush`, controller `enqueue`, `error`,
  `terminate`, and failure propagation preserve ordering and bounded queues.
- Constructor identities survive worker bootstrap refreshes and remain
  distinct from page-realm constructors. Existing body ownership, transfer,
  cancellation, command, and stream limits remain authoritative.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-394.md`

## Tradeoffs and explicit follow-up

This ports the already-bounded page-side composition owner into the worker
realm. It provides deterministic in-realm composition and cancellation but
does not claim a resident event loop, true host streaming backpressure, full
Web IDL descriptor parity, cross-source task ordering, or final Core Web
Profile certification. Those remain issue #40 gates.

## Delivered behavior

- Ported the bounded page stream-composition owner into dedicated workers
  with persistent hidden constructor identities for `WritableStream`
  and `TransformStream`.
- Added worker writable sink lifecycle, serialized writes, close/abort
  propagation, bounded queue accounting, writer locking, desired size,
  closed settlement, and release behavior.
- Added worker `ReadableStream.prototype.pipeTo()` and
  `pipeThrough()` with source/destination lock validation, abort
  signals, prevent flags, ordered writes, cancellation, and terminal lock
  release.
- Added worker transform controller lifecycle and readable/writable pairing,
  including start, transform, flush, enqueue, error, terminate, and failure
  propagation.
- Added a local content-worker witness covering constructor exposure,
  ordered writes and close, sink failure with abort and source cancel,
  lock release, transform identity, transformer ordering, and final output.

## Verification

Validation completed locally against this checkpoint:

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_worker_streams_compose_through_bounded_pipes --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_local_worker --locked -- --nocapture` — 9 passed
- `cargo test --quiet -p glass-browser --test native_engine native_local_streams --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_local_transform_streams --locked -- --nocapture` — 1 passed

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
