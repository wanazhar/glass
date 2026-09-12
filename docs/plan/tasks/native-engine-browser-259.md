# Glass native engine browser slice 259: Fetch Request body ownership

Status: completed locally.

## Objective

Make native `Request` objects obey the core single-use body ownership rule so
Fetch callers can safely observe, clone, and consume request bodies without
silently replaying the same source body.

## Contract

- A native `Request` exposes bounded `bodyUsed` state. Fresh Requests and
  pre-consumption clones report `false`.
- The first `fetch(request)` that uses a non-null source body claims that
  Request's body. A later fetch using the same source Request rejects with a
  `TypeError` before transport dispatch.
- `new Request(usedRequest)` and `usedRequest.clone()` reject with a
  `TypeError`. Clones made before consumption retain independent body state.
- `fetch(request, { body: replacement })` explicitly supplies a replacement
  body and does not claim or re-claim the source Request body.
- Requests without bodies, including `GET` and `HEAD`, retain reusable
  `bodyUsed: false` behavior.
- Existing method, header, redirect, mode, payload-size, and transport bounds
  remain authoritative. Full Request body streams, convenience body methods,
  and complete Fetch Streams/Web IDL semantics remain open.

## Implementation

`RequestNative` now owns a hidden mutable body state behind the frozen public
object and exposes a read-only `bodyUsed` getter. Construction from an already
used Request and cloning after use fail closed. `fetchNative` identifies the
source-body case by checking whether the options object explicitly owns
`body`, claims the source state once, and leaves explicit replacement bodies
independent.

The integration witness serves an HTML page and three bounded POSTs. It checks
fresh and cloned state, successful first consumption, reuse and construction
rejection, explicit replacement-body dispatch, and independent clone
consumption while asserting the exact transmitted `source` and `override`
payloads.

## Tradeoffs and follow-up

The body state is deliberately narrow and compatible with the current bounded
string/blob/byte payload bridge. It does not yet expose a Request
`ReadableStream`, Request body convenience methods, streaming uploads, or
full Web IDL identity. Marking the source body before dispatch prevents a
caller from replaying a body across asynchronous transport work, while an
explicit options-body override is retained as a clear opt-in replacement
boundary.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_tracks_request_body_disturbance_and_clone_ownership -- --nocapture --exact` (1 passed)
- `git diff --check`

The evidence is local-only. Remote CI, push, release, registry publication,
and final native/CDP parity claims remain pending the wider issue #40 gates.
