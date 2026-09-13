# Glass native engine browser slice 288: worker response streams

Status: completed locally.

## Objective

Expose the classic dedicated-worker Fetch response body as a real
byte-preserving `ReadableStream`. Worker code must be able to read, cancel,
iterate, and BYOB-read the response body while preserving the existing
one-shot response ownership and clone semantics.

## Contract

- Worker Fetch responses expose `body` as a `ReadableStream`, and the stream
  passes `instanceof ReadableStream` in the worker realm.
- Default readers produce bounded `Uint8Array` chunks; BYOB readers fill caller
  views without changing the underlying bytes.
- Reader lock/release, `closed`, cancellation, async iteration, and stream
  disturbance are observable and bounded. Reading or cancelling the stream
  marks the owning response `bodyUsed`.
- `text()`, `json()`, `arrayBuffer()`, `bytes()`, and `blob()` remain one-shot
  consumers of the same body; `clone()` remains independently readable before
  either response is disturbed or locked.
- The stream is built from the already bounded host response snapshot. The
  shared Rust loader remains the URL, cookie, CORS, redirect, transport, and
  response-limit owner; no CDP fallback or page resolver is introduced.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The worker bootstrap installs a stable bounded `ReadableStream` constructor
for response byte snapshots. Its byte reader supports fixed-size default
chunks and caller-provided BYOB views, while lock ownership, cancellation,
async iteration, and stream disturbance remain in the worker realm. Response
convenience methods share the stream's body state, and response clones receive
fresh independent streams from the serialized payload.

This slice intentionally keeps transport buffering unchanged: worker Fetch
still resolves through the existing host-buffered loader response. That
extends the worker-facing API without duplicating content-process stream
ownership or introducing a second network policy path.

## Tradeoffs and follow-up

The buffered stream adds no extra transport round trips, but it does not yet
provide demand-driven worker transport or chunk delivery while the host request
is in flight. Full `Request`/`Response` Web IDL identity, user-created worker
stream sources, worker XHR, module/shared/service workers, transfer lists, and
complete native/CDP parity remain Issue #40 work.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet --locked -p glass-browser --features native-engine --test native_engine native_content_process_worker_fetch_preserves_binary_request_and_response_bodies -- --nocapture` (1 passed)
- `cargo test --quiet --locked -p glass-browser --features native-engine --test native_engine worker -- --nocapture` (13 passed)
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
