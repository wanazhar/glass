# Glass native engine browser slice 266: stream-backed Response bodies

Status: completed locally.

## Objective

Allow page code to construct a native `Response` from a readable stream and
consume or clone that body through the same bounded ownership model used by
Fetch responses.

## Contract

- `new Response(readableStream)` accepts an unlocked, undisturbed native
  stream and exposes it as `response.body`.
- Locked or disturbed stream inputs reject explicitly at construction.
- `Response.clone()` tees an unconsumed stream body, keeps one branch on the
  source, and gives the other branch to the clone.
- `text()`, `json()`, `blob()`, `arrayBuffer()`, and `bytes()` drain a
  constructed stream body as bounded byte-compatible chunks and preserve
  one-shot `bodyUsed`/lock behavior.
- Fetch-created Responses retain their existing static and transport-backed
  response-body ownership paths.

## Implementation

Response payloads now carry an internal stream marker when constructed from a
native `ReadableStream`. The Response body is exposed through mutable private
state behind a frozen public object, allowing clone-through-tee to replace the
source branch. Stream body consumers use the bounded byte collector with a
response-specific consumption flag, so internal draining is allowed while
direct reads after body consumption reject. Queued source values are retained
for constructed streams while the existing static Fetch path can continue to
discard its duplicate queue after claiming the payload snapshot.

## Tradeoffs and follow-up

Constructed Response bodies are buffered by the JavaScript boundary when a
convenience method is used; this does not yet provide streaming transforms,
BYOB ownership, custom queuing strategies, piping, transfer, or full Web IDL
parity. Fetch transport Responses continue to use their dedicated incremental
owner. These remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_response_objects_support_stream_bodies_and_tee --locked -- --nocapture --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine response --locked -- --test-threads=1 --nocapture` (5 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine fetch --locked -- --test-threads=1 --nocapture` (19 passed)
- `git diff --check`

The evidence is local-only. Remote CI, release, registry publication, and
final native/CDP parity claims remain pending the wider issue #40 gates.
