---
id: native-engine-browser-855
scope: glass-browser/native-engine/xhr-denied-credentialed-preflight
status: in-progress
depends-on: [native-engine-browser-854]
---

# Glass native-engine browser slice 855: deny credentialed XHR preflight

## Objective

Add process-backed negative CORS-preflight coverage proving that an
unauthorized credentialed XHR fails before its actual request is dispatched.
Keep cookie selection and response-cookie processing exclusively in the
parent.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#xmlhttprequest-credentials`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-854.md` — successful XHR preflight.
- `docs/plan/tasks/native-engine-browser-843.md` — parent broker and
  fail-closed contract.
- `docs/architecture/native-engine.md` — parent CORS/network owner.
- [Fetch Standard: CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- A cross-origin page or DedicatedWorker XHR with `withCredentials == true`
  and a preflighted request sends an OPTIONS request without cookies.
- If the preflight response uses wildcard `Access-Control-Allow-Origin`, it
  cannot authorize a credentialed request even when
  `Access-Control-Allow-Credentials: true` is present.
- The XHR reports a network error and the parent sends no actual POST. The
  process-backed fixture must fail if any actual request reaches its listener.
- Any `Set-Cookie` on the rejected cross-origin preflight response is not
  accepted. Existing parent jar state remains intact and HttpOnly values stay
  absent from `document.cookie`.
- Exercise both page asynchronous XHR and DedicatedWorker asynchronous XHR.
  The parent remains the sole cookie matcher, `Set-Cookie` acceptor, and jar
  owner; no raw cookie headers or jar cross IPC, and no direct child HTTP(S)
  retry is allowed.
- This negative regression is not complete CORS, XHR/Web IDL or WPT
  conformance, cross-platform certification, or remote CI.

## Path

- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-855.md`
- `docs/plan/reviews/native-engine-browser-855-01.md`

## Verification

- Implement one bounded two-origin process regression. The API fixture accepts
  only the expected OPTIONS requests and detects any follow-up actual request.
- Run scoped `cargo check` for `glass-browser` and the affected integration
  target before the exact test. Use the shared
  `/home/ubuntu/work/glass/target`; do not run workspace-wide tests.
- Run the exact process-backed regression once; no adjacent tests unless
  production request/owner code changes.
- Run formatting, `git diff --check`, and the four maintainer documentation
  gates after final docs edits.
- Directly review the CORS denial and parent cookie boundary; no independent
  agent review is used.
- Record local-only evidence. No remote CI or broad conformance claim is
  implied.
