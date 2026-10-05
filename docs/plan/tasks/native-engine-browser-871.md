---
id: native-engine-browser-871
scope: glass-browser/native-engine/dedicated-worker-websocket-failed-handshake-cookies
status: done
depends-on: [native-engine-browser-869, native-engine-browser-870]
---

# Glass native-engine browser slice 871: DedicatedWorker WebSocket failure cookies

## Objective

Verify the process-backed DedicatedWorker WebSocket owner path applies an
eligible HttpOnly `Set-Cookie` from a rejected HTTP handshake in the browser
parent, and that the cookie is reused by a later authorized request without
exposing it to the worker or `document.cookie`.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/architecture/native-engine.md` — process-backed page and Worker
  WebSocket ownership
- `docs/plan/tasks/native-engine-browser-869.md` — rejected page-handshake
  response cookies in the process-backed IPC owner
- `docs/plan/tasks/native-engine-browser-870.md` — rejected browser-owned
  SharedWorker handshake response cookies in its separate parent coordinator
- [WebSockets Standard: opening handshake](https://websockets.spec.whatwg.org/#opening-handshake)
- [Fetch Standard: HTTP-network fetch](https://fetch.spec.whatwg.org/#http-network-fetch)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Network-owner crosswalk

| Request class | Owner and existing evidence | Slice 871 treatment |
|---|---|---|
| Rejected page WebSocket handshake | Process-backed parent IPC handler; Slice 869 proves parent cookie acceptance and later page reuse. | Preserve as baseline. |
| Rejected DedicatedWorker WebSocket handshake | Same parent IPC handler with Worker initiator and worker ID validation; successful text/binary/close behavior is covered, but no failed-handshake cookie regression covers the Worker owner. | Verify its handshake cookie selection, rejected-response cookie acceptance, bounded error delivery, and page reuse. |
| Rejected browser-owned SharedWorker handshake | Separate parent coordinator; Slice 870 verifies its failed-handshake response cookie path. | Preserve; do not conflate the two owners. |

## Contract

- Serve a page and Worker script from one loopback HTTP origin and WebSocket
  endpoints from a separate loopback origin. The page response sets a
  parent-owned HttpOnly seed cookie.
- The page first opens a WebSocket that receives HTTP 403 with an HttpOnly
  response cookie. Its error event creates a DedicatedWorker, so the test
  proves the page response cookie was committed before the Worker request.
- The Worker opens a second WebSocket. Its handshake must carry the seed and
  the page-handshake response cookie. The API returns HTTP 403 with a distinct
  HttpOnly Worker-handshake cookie.
- The Worker reports bounded error/close lifecycle state to the page, receives
  no `open`, and cannot observe either cookie. The parent applies the Worker
  response cookie before dispatching those events.
- A later authorized page Fetch carries all three cookies. The parent cookie
  API reports all as HttpOnly and `document.cookie` remains empty.
- Keep response headers, raw cookie values, and the complete jar in the
  parent. No WebSocket retry or redirect is permitted. A timeout or transport
  error without an HTTP response must not synthesize a cookie update.

## Tradeoff

This is process-backed owner evidence for one sequential page/Worker pair of
HTTP 403 handshakes. It does not expand WebSocket redirect behavior, cookies
for failures without HTTP responses, invalid-101 handling, or full
WebSocket/WPT/cross-platform conformance.

## Path

- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/architecture/native-engine.md`
- `docs/plan/tasks/native-engine-browser-871.md`
- `docs/plan/reviews/native-engine-browser-871-01.md`

## Verification

- Extend the process-backed failed-handshake regression to serialize a page
  WebSocket failure followed by a DedicatedWorker WebSocket failure; assert
  each request's selected cookies, both 403 response cookies, no `open`, no
  retry/redirect, later authorized reuse, HttpOnly filtering, and empty
  `document.cookie`.
- Run one scoped `cargo check` for `glass-browser` and the integration target
  before the exact regression. Reuse `/home/ubuntu/work/glass/target` and
  suppress successful compiler output.
- Run only the exact regression, then Rust formatting, `git diff --check`, and
  all four maintainer documentation gates after final docs edits.
- Commit the design checkpoint before code and evidence, then commit the
  completed slice locally using a focused Conventional Commit. Do not push or
  claim remote CI.
- Update Issue #40 after the local implementation checkpoint. Keep the epic
  open.

## Results

The existing parent IPC handler already accepted the response cookie for a
Worker-owned WebSocket; the missing piece was process-backed evidence for that
owner. The regression now opens a page WebSocket, receives a 403 with an
HttpOnly cookie, and creates a DedicatedWorker from the page's error event. The
Worker handshake carries the seed and page response cookie, receives its own
403 response cookie, and reports an error without an `open` event. A later
authorized page Fetch carries all three cookies; the parent API reports each
as HttpOnly and `document.cookie` remains empty. No retry or redirect occurs.

The first test attempt waited for the Worker handshake response before running
another browser operation. That prevented the queued page WebSocket error
event from being dispatched, so the Worker was never created. The regression
now advances the owner turn by polling page/Worker state before waiting on the
server response. This was a test-harness ordering defect; no runtime code
change was required.

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-browser --features native-engine --lib --test native_engine --locked --quiet` passed. Cargo emitted existing dead-code warnings in the legacy HTML parser and unused helper paths.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-browser --features native-engine --test native_engine --locked --quiet native_content_process_page_and_worker_websocket_failed_handshakes_keep_parent_cookies -- --exact --nocapture` passed (1 passed; 933 filtered; 20.52 seconds).
- `rustfmt --edition 2024 crates/glass-browser/tests/native_engine.rs` and `git diff --check` passed.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-871.json` passed (1,525 Markdown documents; 83 current; zero current-claim failures).
- `python3 scripts/check-documentation-depth.py` passed (93 guides and 19 contracts).
- `python3 scripts/check-tui-shortcuts.py` passed (15 implementation keys and 63 documentation markers).
- `python3 scripts/check-documentation-coverage.py --glass /home/ubuntu/work/glass/target/debug/glass --glass-browser /home/ubuntu/work/glass/target/debug/glass-browser` passed (1,525 Markdown files; 346 full-product MCP tools, including 101 browser-only; 17 examples; 22 public modules). It used the existing binaries in the shared target directory.

This is focused local owner evidence only. It does not claim WebSocket/WPT
conformance, cross-platform certification, remote CI, or browser completion.
Issue #40 remains open.
