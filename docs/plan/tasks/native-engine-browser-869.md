---
id: native-engine-browser-869
scope: glass-browser/native-engine/websocket-failed-handshake-response-cookies
status: in-progress
depends-on: [native-engine-browser-868]
---

# Glass native-engine browser slice 869: failed WebSocket handshake response cookies

## Objective

Verify that the parent accepts an eligible HttpOnly `Set-Cookie` from a
credentialed WebSocket handshake response before a non-101 status fails the
connection, and that a later authorized request reuses the parent-owned cookie.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-843.md` — successful parent-owned
  SharedWorker WebSocket handshake, frames, close, and cookie reuse.
- [WebSockets Standard: opening handshake](https://websockets.spec.whatwg.org/#opening-handshake)
- [Fetch Standard: HTTP-network fetch](https://fetch.spec.whatwg.org/#http-network-fetch)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Network-owner crosswalk

| Request class | Owner and existing evidence | Slice 869 treatment |
|---|---|---|
| Successful SharedWorker WebSocket handshake | Parent coordinator; Slice 843 verifies request and 101 response cookies, frames, and lifecycle. | Preserve the successful-upgrade path. |
| Rejected WebSocket handshake | Same parent `connect_async` path; no process-backed non-101 response-cookie evidence was found. | Preserve the HTTP error response headers long enough to apply eligible cookies before returning the socket error. |
| Later page request and script surface | Parent matcher/jar; page receives only a URL-scoped visible projection. | Verify later reuse and HttpOnly filtering without exposing the rejected response. |

## Contract

- Serve a page and API on distinct loopback origins. The page response sets an
  HttpOnly seed cookie and opens a WebSocket to the API.
- The parent-selected seed is present on the WebSocket handshake. The API
  rejects the handshake with HTTP 403 and a distinct HttpOnly `Set-Cookie`.
  The WebSocket reports failure without an `open` event; it does not follow a
  redirect or retry.
- Before reporting the connection error, the parent accepts the response
  cookie into the same context jar used for later page requests.
- A later explicitly credentialed page Fetch carries the seed and the
  response cookie. The parent cookie API reports both as HttpOnly, while
  `document.cookie` remains empty.
- Keep the response, raw `Set-Cookie` header, and complete jar in the parent.
  The content process receives only bounded failure/lifecycle data.
- Process cookies only when an HTTP response exists. A connect timeout, TLS
  failure, or transport error without response headers must not synthesize a
  response-cookie update. Preserve the existing success and malformed-response
  error behavior.

## Standards and implementation evidence

The WebSockets Standard defines the handshake through Fetch with credentials
mode `include`, then fails the WebSocket when the response status is not 101.
Fetch's HTTP-network response processing handles response cookies before that
WebSocket status check. In the locked `tokio-tungstenite` 0.26.2 source, a
non-101 response is retained in `tungstenite::Error::Http`, including its
headers, so the parent can apply `Set-Cookie` without moving authority across
IPC.

## Tradeoff

This slice covers a real HTTP 403 response to one page WebSocket handshake. It
does not claim WebSocket redirect support, TLS/network-failure cookies,
invalid-101 handshake-header behavior, full WebSocket/WPT conformance, or
cross-platform parity.

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-869.md`
- `docs/plan/reviews/native-engine-browser-869-01.md`

## Verification

- Add one process-backed real-loopback regression for a rejected handshake.
  Assert seed-cookie selection, status failure with response-cookie acceptance,
  no WebSocket open/retry/redirect, later authorized cookie reuse, and
  HttpOnly filtering from `document.cookie`.
- Run one scoped `cargo check` for `glass-browser` and the integration target
  before the exact regression. Reuse `/home/ubuntu/work/glass/target`; suppress
  successful compiler output.
- Run only the exact regression, then formatting, `git diff --check`, and all
  four maintainer documentation gates after final documentation edits. Use the
  existing shared-target binaries by explicit path.
- Commit the design checkpoint before code and evidence. Commit the completed
  slice locally with a focused Conventional Commit; do not push or claim remote
  CI.
- Update Issue #40 only after the local checkpoint; keep the epic open.

## Results

Implementation and focused verification are pending.
