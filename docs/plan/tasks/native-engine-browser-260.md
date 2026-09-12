# Glass native engine browser slice 260: Fetch Request body surface

Status: completed locally.

## Objective

Expose the bounded native Request body surface needed by ordinary browser
code: a readable body stream, one-shot body consumers, and stable payload
ownership from construction through Fetch dispatch.

## Contract

- Every non-null Request body is represented by a bounded native
  `ReadableStream`; body-less Requests retain `body === null`.
- `Request.bodyUsed` becomes true when a body consumer, stream read, stream
  cancel, or Fetch source-body handoff disturbs the body.
- `text()`, `json()`, `blob()`, `arrayBuffer()`, and `bytes()` consume the body
  once and reject subsequent consumption. `blob()` preserves the captured
  Request content type.
- A locked or disturbed body rejects Request cloning and construction from
  that Request. A clone created before use receives an independent byte
  stream and captured payload snapshot.
- Fetch uses the captured Request bytes for source-body dispatch, including
  Blob, File, FormData, URLSearchParams, ArrayBuffer, and typed-array input;
  explicit options-body overrides remain independent.
- The existing size, method, header, encoding, security, and transport bounds
  remain authoritative. Caller-supplied streaming uploads, `Request.formData`,
  and complete Fetch Streams/Web IDL parity remain open.

## Implementation

The native stream state now supports an ownership disturbance callback and a
Request-consumed marker. `RequestNative` captures a bounded byte payload,
installs a static stream with independent state, exposes body methods, and
reuses the captured snapshot when cloning or dispatching through Fetch.
Request stream reads and cancellation feed the existing single-use checks.

The integration witness covers text, JSON, binary bytes, Blob type/size,
pre-use clone independence, stream reads and completion, locked clone
rejection, body-used state, and empty-body text consumption.

## Tradeoffs and follow-up

The body stream is a bounded snapshot rather than a caller-provided streaming
upload source. This gives deterministic ownership and avoids retaining mutable
FormData/Blob inputs after Request construction, but it does not yet model
full streaming upload backpressure, BYOB readers, piping, multipart
`formData()` parsing, or the complete Body/Web IDL algorithms. Those remain
explicit follow-up work rather than being represented as supported by the
native capability table.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_request_body_stream_and_methods --locked -- --nocapture --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine request_body --locked -- --test-threads=1 --nocapture` (2 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine fetch --locked -- --test-threads=1 --nocapture` (18 passed)
- `git diff --check`

The evidence is local-only. Remote CI, push, release, registry publication,
and final native/CDP parity claims remain pending the wider issue #40 gates.
