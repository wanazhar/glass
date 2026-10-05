# Native engine Slice 849 review

- **Revision reviewed:** Slice 849 candidate based on `e160cba7`
- **Task:** `native-engine-browser-849`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

None. No blocking contract mismatch was found.

## Review notes

- The regression exercises a real content-worker DedicatedWorker timer turn,
  not a page-triggered substitute: it waits for the worker's async-effect
  notification and dispatches that exact context/frame owner turn. The loopback
  server checks request order, `POST`, `Content-Type`, and the exact uploaded
  body (`crates/glass-browser/src/browser/native_engine/engine.rs:11601-11841`).
- The request-cookie sequence is parent-owned: an owner-bound visible page
  setter and parent-accepted HttpOnly cookies from the page, Worker entry, and
  imported dependency are present on later requests. The parent-accepted
  HttpOnly rotation from the first timer Fetch is present on the upload, and
  the upload's `Set-Cookie` is present on the following Fetch and parent cookie
  API. `document.cookie` excludes HttpOnly cookies.
- The existing broker still validates the exact context, frame, generation,
  and document URL before accepting script Fetches. Owner-tagged cookie writes
  are checked against that owner; the parent broker selects request cookies,
  receives response-cookie changes, and returns only the bounded visible
  cookie projection (`content_process.rs:6668-6677, 6710-6756, 7635-7666,
  1080-1199`). Slice 849 changes none of these runtime checks.
- Upload pull data crosses the existing bounded demand-driven upload protocol
  and is collected before parent dispatch (`content_process.rs:23190-23290`).
  The task and design docs explicitly avoid claiming socket-level upload
  streaming or network backpressure. Autonomous ServiceWorker upload coverage,
  stale-owner/cancellation-specific tests, WPT, and remote CI remain open.
- The initial scoped check exposed a pre-existing unit-test import error for
  `NativeCookieProfileEntry`; the test-only import now comes from its defining
  `native_engine` re-export (`native_backend.rs:11703-11718`). No production
  runtime behavior changed in that repair.
- The scoped library/test check, companion worker build, and two timer-cookie
  tests pass locally. This is not an independent review or remote-CI result.

## Conclusion

**Pass (direct self-review; not an independent review).**
