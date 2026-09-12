# Glass native engine browser slice 265: stream-backed Request bodies

Status: completed locally.

## Objective

Connect page-created native `ReadableStream` instances to the existing
Request/Fetch body contract so browser code can produce uploads incrementally
at the JavaScript boundary while retaining the native transport's bounded
request policy.

## Contract

- `new Request(url, { method, body: readableStream })` accepts a usable native
  stream and exposes that stream as `request.body`.
- Locked or disturbed stream inputs reject at Request construction; Request
  body disturbance and consumption remain one-shot.
- Request `text()`, `json()`, `blob()`, `arrayBuffer()`, `bytes()`, and
  `formData()` drain stream chunks, accepting bounded strings and byte-buffer
  values and rejecting other values or bodies over the native request limit.
- `new Request(existingStreamRequest)` and `existingStreamRequest.clone()`
  tee an unconsumed body, retaining one branch on the source and one on the
  clone.
- Fetch drains a Request-owned stream or direct options-owned stream before
  sending the existing Rust request command, preserving method, headers,
  content type, abort, and response behavior.

## Implementation

Request body payloads now distinguish static bytes from an owned stream. The
Request body property is backed by mutable internal state so a pre-consumption
clone can replace the source with its tee branch despite the public Request
object being frozen. A bounded native stream collector reads one chunk at a
time, normalizes strings/ArrayBuffers/views to bytes, enforces the existing
form/request body limit, releases its reader, and hands the resulting bytes to
the established Fetch command serializer. Internal body draining has an
explicit ownership state so the collector can read a claimed body while later
consumer attempts still reject.

## Tradeoffs and follow-up

The JavaScript-to-Rust boundary buffers the complete upload before issuing the
request; it does not yet provide transport-level upload streaming, upload
progress, duplex request semantics, or BYOB ownership. Stream chunks are
restricted to bounded byte-compatible values so arbitrary JavaScript objects
cannot be stringified into an accidental request body. Full Streams/Web IDL
parity, piping, transfer strategies, and upload progress remain issue #40
work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_fetches_stream_request_bodies_and_clones_them --locked -- --nocapture --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine request_body --locked -- --test-threads=1 --nocapture` (2 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine tee --locked -- --test-threads=1 --nocapture` (3 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine fetch --locked -- --test-threads=1 --nocapture` (19 passed)
- `git diff --check`

The evidence is local-only. Remote CI, release, registry publication, and
final native/CDP parity claims remain pending the wider issue #40 gates.
