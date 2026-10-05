---
id: native-engine-browser-859
scope: glass-browser/native-engine/xhr-cors-error-response-cookies
status: complete
depends-on: [native-engine-browser-858]
---

# Glass native-engine browser slice 859: retain parent-owned cookies on CORS errors

## Objective

Verify that a credentialed cross-origin XHR response can fail CORS visibility
while its `Set-Cookie` is still accepted by the parent-owned cookie jar, as
specified by Fetch. Fix the response-cookie commit ordering if the process
regression confirms the current error path drops it.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-843.md` — parent broker contract.
- `docs/plan/tasks/native-engine-browser-857.md` — credential-mode preflight
  cache isolation.
- `docs/architecture/native-engine.md` — parent CORS/network owner.
- [Fetch Standard: CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- A valid credentialed preflight is sent without Cookie; any `Set-Cookie` on
  that preflight response is ignored.
- The actual cross-origin POST carries the matching seed cookie selected by
  the parent. Its response has `Access-Control-Allow-Origin: *` and
  `Access-Control-Allow-Credentials: true`, so XHR reports a network error
  and exposes neither response status nor body to script.
- The actual response's HttpOnly `Set-Cookie` is nevertheless processed by
  the parent cookie owner. A subsequent credentialed request carries both the
  seed and response cookies, proving persistence without exposing either
  HttpOnly cookie through `document.cookie`.
- No raw cookie header, cookie jar, or cookie-matching authority crosses IPC;
  the content process cannot retry the request directly.
- Cookie acceptance is independent of script access to the response. This
  slice covers this focused XHR/network boundary, not complete Fetch/CORS,
  XHR/Web IDL or WPT conformance, cross-platform certification, or remote CI.

## Tradeoff

Rejecting the response for script visibility must not discard browser-owned
network side effects that Fetch still permits. Preserving those effects in the
parent maintains cookie authority, while XHR still fails closed and cannot
observe the response. The test must separately prove that rejected preflight
cookies remain ignored.

## Path

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-859.md`
- `docs/plan/reviews/native-engine-browser-859-01.md`

## Verification

- Add one bounded two-origin, process-backed credentialed XHR regression. The
  fixture distinguishes preflight, CORS-failed actual response, and a later
  authorized request, and asserts each Cookie header and response-cookie
  outcome.
- Implement the smallest parent-side response-cookie ordering fix only if the
  regression demonstrates the gap. Keep the cookie jar, `Set-Cookie`
  parsing/acceptance, and persistence parent-owned.
- Run scoped `cargo check` for `glass-browser` and the affected integration
  target before the exact test, using shared `/home/ubuntu/work/glass/target`.
  Do not run workspace-wide tests.
- Run the exact process-backed regression once; do not repeat adjacent tests
  unless production request or owner code changes.
- Run formatting, `git diff --check`, and all four maintainer documentation
  gates after final docs edits.
- Record local-only evidence. No remote CI or broad conformance claim is
  implied.

## Results

- The CORS-error branch now commits pending response cookies to the
  `NativeResourceLoader` parent-owned jar before returning the script-visible
  network error. The normal successful-response path is unchanged.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p
  glass-browser --features native-engine --test native_engine --locked
  --quiet` passed with existing dead-code warnings.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p
  glass-browser --features native-engine --test native_engine --locked
  --quiet native_content_process_xhr_keeps_parent_cookie_on_cors_error --
  --exact` passed (1 passed; 923 filtered; 20.42 seconds).
- The process fixture observed one cookie-free valid OPTIONS preflight with a
  rejected preflight `Set-Cookie`; the first POST carried only the seeded
  parent cookie and received a wildcard-origin response cookie. Script saw
  `error`, status 0, and an empty body; the next POST carried the seed and
  `cors_failed=accepted`. The parent cookie API retained both as HttpOnly,
  while `document.cookie` remained empty.
- Cookie storage and matching stayed in the parent. No raw cookie header or
  jar was added to IPC, and no child-side request retry was enabled.
- Formatting and maintainer documentation gates are recorded in the Slice
  859 review. This is local focused evidence only; it does not certify full
  CORS/XHR or WPT conformance, other platforms, independent review, or remote
  CI. Issue #40 remains open.
