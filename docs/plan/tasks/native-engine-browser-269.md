# Glass native engine browser slice 269: byte streams and BYOB

Status: completed locally.

## Objective

Complete the next shared Streams contract needed by Fetch, Request, Response,
tee, and piping consumers: byte-source validation, bounded queuing-strategy
accounting, and BYOB reads that preserve caller-owned buffers across partial
delivery.

## Contract

- `new ReadableStream({ type: "bytes", ... }, strategy)` creates a bounded
  byte stream; ordinary object streams remain distinct and reject BYOB readers.
- `highWaterMark` is validated, capped by the native stream bound, and exposed
  through controller `desiredSize`; a supplied `size()` algorithm is called for
  queued chunks and invalid sizes fail explicitly.
- `getReader({ mode: "byob" })` accepts a non-empty typed-array/DataView view
  only for byte streams and returns byte ranges from that caller-provided view.
- A byte-source controller exposes `byobRequest`, whose `view`, `respond()`,
  and `respondWithNewView()` methods settle the pending read through the same
  bounded owner; partial chunks remain queued for the next read.
- Fetch transport chunks and `tee()` branches preserve byte mode, while close,
  error, cancellation, and body ownership continue to settle existing default
  readers correctly.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-269.md`

## Implementation

The shared bootstrap now keeps byte-mode state separate from ordinary stream
state, validates byte chunks before enqueue, records bounded queue sizes, and
computes strategy-aware `desiredSize`. A BYOB reader stores one pending view;
the byte controller projects it through a frozen `byobRequest` and supports
both response forms. Reads consume only the available aligned bytes and put a
remainder back into the bounded queue. Fetch event delivery uses the same
pending-read path, and tee-created branches inherit byte mode.

The implementation retains the native finite queue and transfer limits. The
strategy value is capped at the native queue bound, and the transport handoff
continues to buffer at the existing bounded boundary rather than pretending
to provide arbitrary zero-copy or transferable backing storage.

## Tradeoffs and follow-up

This slice covers the common byte-stream/BYOB contract needed by current native
Fetch and page-created stream flows. It does not yet claim complete Streams
specification behavior, transferable streams, detached-buffer choreography,
custom byte queuing strategies beyond the bounded `highWaterMark`/`size`
surface, upload progress, or full Web IDL descriptor/brand parity. Those
remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_byte_streams_support_byob_reads_and_strategies --locked -- --nocapture --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine stream --locked -- --test-threads=1 --nocapture` (13 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine response --locked -- --test-threads=1 --nocapture` (5 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine fetch --locked -- --test-threads=1 --nocapture` (19 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine request_body --locked -- --test-threads=1 --nocapture` (2 passed)
- `python3 scripts/check-release-documentation.py --require-previous-version`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
