# Native worker stream body ownership (393)

```yaml
id: native-engine-browser-393
scope: native-engine/worker-stream-body-ownership
status: done
depends-on:
  - native-engine-browser-392
```

## Objective

Close the worker Fetch Body gap left by the worker ReadableStream source and
transfer slices. Worker Request and Response objects must retain native
source-backed or transferred streams, consume them asynchronously, and keep
clone ownership independent instead of rejecting them or silently turning them
into empty snapshots.

## Contract

- `new Request(..., { body: readableStream })` retains the stream, and body
  methods drain it through the worker stream owner with the existing bounded
  body limit.
- `Request.clone()` tees an unconsumed stream, rewires the source request to
  one branch, and gives the clone the other branch. `bodyUsed`, locking, and
  one-shot consumption follow the branch currently owned by each Request.
- `fetch(Request)` and `fetch(url, { body: readableStream })` drain the body
  asynchronously before emitting the existing bounded host Fetch command.
  Method, signal, header, and body-limit validation still applies at dispatch;
  an abort observed while draining prevents dispatch.
- `new Response(readableStream)` retains the stream. Response body methods
  drain it asynchronously, `bodyUsed` observes direct stream disturbance, and
  `Response.clone()` tees the stream without aliasing the original body.
- Source-backed, byte-mode, and transferred worker streams remain owned by the
  worker realm. The host boundary continues to receive one bounded byte
  payload, so no CDP or eager empty-body fallback is introduced.

## Delivered behavior

- Added worker stream drain and chunk-to-byte conversion with bounded total
  body accounting and internal consuming flags that allow only the owning body
  operation to read a claimed stream.
- Extended Worker Request construction, body methods, cloning, and fetch
  dispatch to preserve stream payloads and asynchronously drain source-backed,
  byte-mode, and transferred streams.
- Extended Worker Response construction, body methods, cloning, and
  `bodyUsed` to preserve source-backed streams and use the same bounded drain
  owner.
- Added content-process worker coverage for source-backed Request and Response
  bodies, clone independence, and both Request-source and options-body Fetch
  uploads, including server-side byte assertions.

## Tradeoffs and explicit follow-up

This slice preserves stream semantics through the worker realm but still
buffers the bounded body before crossing the existing JSON-framed host Fetch
command. That keeps the current Rust protocol stable and limits memory use,
but it is not true chunked upload or network backpressure. A streaming host
upload protocol, full worker pipe/TransformStream integration, broader
transferables, cross-source task arbitration, and final Core Web Profile and
production certification remain issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-393.md`

## Verification

Validation completed locally against this checkpoint:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker --locked -- --nocapture` — 9 passed
- `cargo test --quiet -p glass-browser --test native_engine native_local_response_objects_support_stream_bodies_and_tee --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_fetches_stream_request_bodies_and_clones --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_tracks_request_body_disturbance_and_clone_ownership --locked -- --nocapture` — 1 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
