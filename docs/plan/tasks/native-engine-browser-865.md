---
id: native-engine-browser-865
scope: glass-browser/native-engine/shared-worker-eventsource-cors-error-cookies
status: done
depends-on: [native-engine-browser-864]
---

# Glass native-engine browser slice 865: SharedWorker EventSource CORS-error cookies

## Objective

Verify that the browser-owned SharedWorker coordinator processes a credentialed
EventSource actual response's HttpOnly cookie in the parent even when CORS
rejects stream exposure, and that a later authorized page request reuses that
parent-owned cookie.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/native-engine-browser-profile.md#page-eventsource-cors-error-response-cookies`
- `docs/architecture/native-engine.md`
- `docs/plan/tasks/native-engine-browser-843.md` — browser-owned SharedWorker
  coordinator and direct parent-loader ownership for EventSource.
- `docs/plan/tasks/native-engine-browser-864.md` — page EventSource CORS-error
  response-cookie fix and process-backed evidence.
- [Fetch Standard: CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Network-owner crosswalk

| Request class | Owner and evidence | Slice 865 treatment |
|---|---|---|
| Page EventSource | Content-process owner-bound parent broker; Slice 864 verifies CORS-error behavior and parent cookie reuse. | Preserve as baseline; no new page IPC route. |
| Browser-owned SharedWorker EventSource | Parent coordinator calls the parent loader directly and shares the exact context jar; existing Slice 843 evidence covers successful stream delivery and cookie reuse, but not an actual response rejected by credentialed CORS. | Add a process-backed CORS-error case for this distinct network owner. |
| Content-process SharedWorker EventSource | Broker-only worker registry; not the browser-owned coordinator path under test. | No change. |
| WebSocket and Fetch | Separate parent coordinator/broker contracts with existing scoped evidence. | No change. |

This is an owner-path regression, not a second implementation of EventSource
transport. Both paths use the parent resource loader, but only the browser-owned
SharedWorker coordinator owns this stream directly.

## Contract

- Serve the page and SharedWorker entry from one loopback origin and the event
  stream from a distinct loopback origin.
- The page sets an HttpOnly seed cookie. The SharedWorker entry response sets a
  second HttpOnly cookie. The browser-owned coordinator selects both from the
  parent context jar when opening `EventSource(api_url, { withCredentials: true })`.
- The API receives one simple GET with the page/worker origin and both
  parent-selected cookies; no preflight is sent. Its actual response includes
  `Content-Type: text/event-stream`, wildcard `Access-Control-Allow-Origin`,
  `Access-Control-Allow-Credentials: true`, stream data, and a distinct
  HttpOnly `Set-Cookie`.
- Credentialed CORS rejects the response. The SharedWorker receives an error
  without an open event or message data; it closes EventSource on that error
  so automatic reconnects cannot obscure the cookie-owner result.
- A later authorized page Fetch carries the seed, SharedWorker-entry, and
  actual-response cookies. The parent cookie API confirms all are HttpOnly,
  while `document.cookie` remains empty.
- Keep connection setup, request-cookie matching, response-cookie processing,
  persistence, and close/cancellation in the parent coordinator. Do not send
  raw cookie headers, cookie profiles, or the complete jar to the worker.
- If the regression exposes a defect, make only the minimum coordinator or
  parent-loader change needed to satisfy this contract.

## Tradeoff

This adds one process-backed negative case to the existing SharedWorker
EventSource coverage. It covers the coordinator's CORS-error delivery and
shared parent-jar visibility without expanding EventSource redirect,
reconnection, backpressure, or full WPT semantics. Closing after the first
error intentionally excludes those lifecycle costs.

## Path

- `crates/glass-browser/src/browser/native_backend.rs` (only if coordinator
  error/close propagation is defective)
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs` (only if
  parent response-cookie processing is defective)
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-865.md`
- `docs/plan/reviews/native-engine-browser-865-01.md`

## Verification

- Add one bounded process-backed two-origin SharedWorker EventSource
  CORS-failure regression. Assert parent-selected cookies, Origin, no
  preflight, error without open/message data, parent response-cookie acceptance,
  and later authorized reuse.
- Run one scoped `cargo check` for `glass-browser` and the integration target
  before the exact test. Reuse `/home/ubuntu/work/glass/target`; do not create
  a worktree-local target or run workspace-wide tests.
- Run only the exact regression. Fix any fixture or implementation issue in a
  coherent batch and rerun the exact test.
- Run formatting, `git diff --check`, and all four maintainer documentation
  gates after the final documentation edit; use the existing shared-target
  `glass` and `glass-browser` binaries by explicit path.
- Commit locally with the configured Git identity. Do not push code or claim
  remote CI, platform certification, full EventSource, or WPT conformance.
  Keep Issue #40 open.

## Results

- The browser-owned coordinator already called the parent
  `open_event_source_async` loader and shared the browser context's parent jar.
  The Slice 864 loader fix therefore applied without another runtime change.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p
  glass-browser --features native-engine --lib --test native_engine --locked
  --quiet` passed; successful compiler output was suppressed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p
  glass-browser --features native-engine --test native_engine --locked
  --quiet native_runtime_shared_worker_event_source_cors_errors_keep_parent_cookies
  -- --exact --nocapture` passed (1 passed; 928 filtered; 22.94 seconds).
- The process-backed regression verifies the actual EventSource GET, Origin,
  parent-selected seed and SharedWorker-entry cookies, no preflight, rejected
  credentialed CORS response, error without open/message events, parent
  acceptance of the actual-response cookie, and later authorized page Fetch
  reuse. The parent cookie API retains all three as HttpOnly and
  `document.cookie` remains empty. EventSource closes on the first error so
  reconnect behavior remains outside this evidence.
- Rust formatting, `git diff --check`, and all four maintainer documentation
  gates passed; detailed outputs are recorded in the Slice 865 review.
- This is focused local evidence only. EventSource redirects/reconnects, full
  Fetch/CORS or WPT conformance, other platforms, independent review, and
  remote CI remain unverified. Issue #40 remains open.
