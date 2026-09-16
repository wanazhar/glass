# Native Service Worker request-body replay (420)

status: complete
scope: native-engine/service-worker-request-replay
issue: 40
depends-on: [native-engine-browser-419]

## Objective

Prove and, if necessary, repair replay of a controlled navigation or Fetch
request body inside the Service Worker boundary. A worker must be able to
clone the incoming Request, consume the original, and send the clone through
the native worker Fetch owner without losing bytes or bypassing body-use
ownership.

## Contract

- A controlled POST request exposes its bounded body to
  `event.request.text()` or `arrayBuffer()`.
- `event.request.clone()` preserves an independent body and can be passed to
  worker `fetch()` after the original request is consumed.
- The replayed request retains method, headers, content type, and exact body
  bytes at the native network boundary.
- Body-use and clone failures remain typed and bounded; this task does not
  claim unbounded bodies, streaming replay, or full Request/Streams Web IDL
  parity.

## Implementation path

- Exercise the existing Service Worker fetch envelope and Worker Request clone
  owner with a real HTTP controlled POST.
- Observe the original body, replay the clone through worker `fetch()`, and
  verify the upstream server sees one exact POST body.
- Preserve the transferred `contentType` when constructing the Service Worker
  Request because the native Fetch command carries it separately from the
  ordinary header map.
- Synchronize architecture, plan, analysis, task, and issue #40.

## Tradeoffs

- The current boundary remains bounded and buffered, so this task validates
  byte-preserving replay rather than streaming backpressure or tee behavior.
- Reusing Worker Request/Fetch ownership keeps body-use semantics in one place
  and avoids a Service Worker-specific bypass of the resource loader.

## Evidence

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_service_worker_replays_cloned_request_body --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine service_worker --locked -- --nocapture` (19 passed)
- `cargo fmt --all`
- `git diff --check`
- documentation coverage, depth, and release-truth checks passed

Remote CI, push, release, tag, and registry publication are outside this
local checkpoint.
