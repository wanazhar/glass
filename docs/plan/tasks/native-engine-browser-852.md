---
id: native-engine-browser-852
scope: glass-browser/native-engine/xhr-cross-origin-credentials-cors
status: in-progress
depends-on: [native-engine-browser-851]
---

# Glass native-engine browser slice 852: cross-origin XHR credentials

## Objective

Add process-backed two-origin coverage for XMLHttpRequest credentials and CORS
through the parent broker. A same-host/different-port loopback target keeps
cookie host/path matching eligible while making request origins distinct.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#xmlhttprequest-credentials`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-823.md` — existing two-origin Fetch
  credentials and redirect behavior.
- `docs/plan/tasks/native-engine-browser-851.md` — XHR mode mapping and
  same-origin parent-cookie regression.
- `docs/architecture/native-engine.md` — parent broker and XHR paths.
- [XHR Standard](https://xhr.spec.whatwg.org/#the-withcredentials-attribute)
- [Fetch Standard credentials modes](https://fetch.spec.whatwg.org/#concept-request-credentials-mode)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- Page asynchronous XHR, page synchronous XHR through the parent broker, and
  DedicatedWorker asynchronous XHR use the captured `same-origin`/`include`
  mode when contacting a different origin.
- With the default `withCredentials == false`, the parent sends no matching
  cookies to the cross-origin target and does not accept that response's
  `Set-Cookie`, even though ordinary cookie host/path matching would match.
- With `withCredentials == true`, the request may carry parent-matched
  cookies and its response cookie may be accepted only when the credentialed
  CORS response authorizes the page origin. Later requests observe accepted
  cookies through the parent loader.
- All seed and response cookies in the test are HttpOnly. The script-visible
  projection remains filtered. IPC carries only existing owner-tagged writes
  and the bounded URL-scoped `document.cookie` projection, never raw
  Cookie/Set-Cookie headers or the full jar.
- Existing exact context/frame/generation/document owner checks remain
  mandatory. The content process must not retry through direct HTTP(S).
- This slice tests direct cross-origin requests, not XHR redirect-chain
  behavior, full CORS/WPT conformance, other browser APIs, or cross-platform
  certification.

## Path

- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-852.md`
- `docs/plan/reviews/native-engine-browser-852-01.md`

## Verification

- Complete the two-origin process-backed regression before invoking Cargo.
- Run scoped `cargo check` for `glass-browser` library and test metadata before
  the exact integration test. Use the shared
  `/home/ubuntu/work/glass/target`; do not run broad workspace tests.
- Run the exact process-backed regression and adjacent Slice 851 XHR parent
  cookie test if the change alters shared request/owner code.
- Run `cargo fmt --all -- --check`, `git diff --check`, and the documentation
  release-truth, depth, shortcut, and coverage checks after the final doc edit.
- Directly review CORS credentials and cookie owner boundaries; no independent
  agent review is used.
- Record local-only evidence. No remote CI or broad conformance claim is
  implied.
