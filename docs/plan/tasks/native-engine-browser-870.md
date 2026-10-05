---
id: native-engine-browser-870
scope: glass-browser/native-engine/shared-worker-websocket-failed-handshake-response-cookies
status: done
depends-on: [native-engine-browser-869]
---

# Glass native-engine browser slice 870: SharedWorker failed WebSocket handshake cookies

## Objective

Make the browser-owned SharedWorker WebSocket coordinator process eligible
cookies from a rejected HTTP handshake response in the parent before it
reports socket failure. Verify a later authorized request reuses the cookie
without exposing cookie state or raw response data to the SharedWorker.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-843.md` — successful parent-owned
  SharedWorker WebSocket handshake, frames, close, and profile reuse.
- `docs/plan/tasks/native-engine-browser-869.md` — distinct process-backed
  page WebSocket failure path; not evidence for the SharedWorker coordinator.
- [WebSockets Standard: opening handshake](https://websockets.spec.whatwg.org/#opening-handshake)
- [Fetch Standard: HTTP-network fetch](https://fetch.spec.whatwg.org/#http-network-fetch)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Network-owner crosswalk

| Request class | Owner and existing evidence | Slice 870 treatment |
|---|---|---|
| Process-backed page/DedicatedWorker WebSocket handshake | Parent IPC broker; Slice 869 covers a page's non-101 response. | Leave unchanged; it is a different owner path. |
| Successful browser-owned SharedWorker WebSocket handshake | `native_backend.rs` parent coordinator; Slice 843 covers handshake cookies, frames, close, and later profile reuse. | Preserve the successful-upgrade path. |
| Rejected browser-owned SharedWorker WebSocket handshake | `open_shared_worker_websocket` in `native_backend.rs`; generic `Error` branch discards retained HTTP response headers. | Process eligible `Set-Cookie` in the coordinator before bounded error and close events. |
| Later same-context page request and script surface | Shared parent jar; SharedWorker receives only events, while the page has its scoped visible projection. | Verify request reuse, parent HttpOnly state, and an empty `document.cookie`. |

## Contract

- Use a process-backed page and browser-owned SharedWorker with a loopback
  HTTP/WebSocket server. Set distinct HttpOnly seed and SharedWorker-entry
  cookies before the worker opens its socket.
- The coordinator selects those parent-owned cookies for the WebSocket
  handshake. The server returns HTTP 403 with a third HttpOnly `Set-Cookie`.
- When `connect_async` returns an HTTP response error, the coordinator applies
  eligible response cookies to its existing shared-context parent loader
  before dispatching the normal WebSocket error and abnormal-close events.
- The worker receives bounded error/close lifecycle events only. The HTTP
  response, response headers, raw cookie values, and complete jar remain in
  the parent; it receives no open event and cannot read the HttpOnly cookie.
- A later authorized page Fetch carries all three cookies. The parent cookie
  API reports them as HttpOnly and `document.cookie` remains empty.
- The fixture must reject retries and redirects. Timeout, TLS, and transport
  errors without an HTTP response must retain the existing no-cookie-update
  behavior. Successful upgrade, frame, and teardown behavior remains intact.

## Standards and implementation evidence

The WebSocket opening handshake uses Fetch with credentials mode `include`.
Fetch processes response cookies from an HTTP response before WebSocket
validates that the status is 101. The locked `tokio-tungstenite` version
retains a non-101 response and its headers as `tungstenite::Error::Http`; the
coordinator can apply its `Set-Cookie` values locally without returning the
response across worker IPC.

## Tradeoff

This slice covers a real HTTP 403 response for one browser-owned SharedWorker
handshake. It does not claim WebSocket redirects, cookies on failures without
HTTP responses, invalid-101 handshake-header behavior, full WebSocket/WPT
conformance, or cross-platform parity.

## Path

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-870.md`
- `docs/plan/reviews/native-engine-browser-870-01.md`

## Verification

- Add one process-backed regression named
  `native_runtime_shared_worker_websocket_failed_handshake_keeps_parent_cookies`.
- Run one scoped `cargo check` for `glass-browser` and `native_engine` before
  that exact regression, reusing `/home/ubuntu/work/glass/target` and
  suppressing successful compiler output.
- Run only the exact regression, then Rust formatting, `git diff --check`, and
  all four maintainer documentation gates after final documentation edits.
- Commit the completed slice locally with a focused Conventional Commit, then
  update Issue #40. Do not push or claim remote CI.

## Results

The browser-owned SharedWorker coordinator now handles
`tungstenite::Error::Http` separately. It applies eligible response cookies
through the coordinator's parent loader before dispatching the existing
bounded WebSocket error and abnormal-close events. The error message includes
only the HTTP status; the raw response and cookie headers remain in the
parent. Timeout and transport-error paths without an HTTP response are
unchanged.

The scoped `cargo check` passed, and the exact
`native_runtime_shared_worker_websocket_failed_handshake_keeps_parent_cookies`
regression passed. It verifies the seed and worker-entry cookies on the
handshake, the 403 response cookie on a later page Fetch, all three as
HttpOnly in the parent cookie API, an empty `document.cookie`, and no open,
retry, or redirect. Rust formatting, `git diff --check`, and all four
maintainer documentation gates passed after the final documentation edits.
No remote CI or cross-platform certification is claimed.
