---
id: native-engine-browser-860
scope: glass-browser/native-engine/fetch-cors-error-cookie-ownership
status: complete
depends-on: [native-engine-browser-859]
---

# Glass native-engine browser slice 860: parent-owned Fetch cookies on CORS errors

## Objective

Extend the request-authority audit from XHR to the page and DedicatedWorker
Fetch routes. Verify that a credentialed actual response rejected by CORS is
still processed by the parent cookie owner, and that those cookies are sent
on the next authorized request.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/architecture/native-engine.md#cookie-authority-and-process-boundary`
- `docs/plan/tasks/native-engine-browser-843.md` — parent broker audit and
  remaining network-class boundary.
- `docs/plan/tasks/native-engine-browser-859.md` — XHR CORS-error cookie
  processing.
- [Fetch Standard: CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Network-owner crosswalk

| Request class | Current owner/evidence | Slice 860 gap |
|---|---|---|
| HTTP(S) navigation, forms, and document resources | Parent loader and owner-checked content broker; coverage is recorded in Slice 843 and follow-ups. | No change. |
| Page and DedicatedWorker XHR | Parent broker; credentials, redirects, preflight, and CORS denials are covered through Slice 859. | XHR proves the shared loader behavior, but not the page/Worker Fetch result path. |
| Page and DedicatedWorker Fetch | Parent broker; request and response cookies stay parent-owned. | Add process evidence for cookies from actual responses whose CORS check rejects Fetch. |
| EventSource and WebSocket | Parent broker/coordinator owns network connections, cookies, and response cookies; Slice 843 records process evidence. | No change. |
| SharedWorker and ServiceWorker Fetch | Parent broker or browser-owned coordinator; owner/lifecycle coverage is recorded in Slice 843 follow-ups. | No change. |

Uncovered HTTP(S) requests must continue to fail closed at the content-loader
transport guard; this crosswalk is not a claim that the complete network audit
or GCWP/WPT conformance is finished. Parent-brokered Fetch response streaming
and backpressure remain an explicit later gap.

## Contract

- Use distinct loopback page and API origins and credentialed, simple GET
  requests so the actual Fetch response—not preflight—is the tested boundary.
- The page Fetch and DedicatedWorker Fetch each send the HttpOnly seed cookie
  selected by the parent. Each API response uses wildcard
  `Access-Control-Allow-Origin` with `Access-Control-Allow-Credentials: true`
  and sets a distinct HttpOnly cookie.
- Both Fetch promises reject with `TypeError`; neither response status nor
  body is visible to script. The next Worker request observes the page
  response cookie, and a later authorized page request observes both response
  cookies, proving parent acceptance and ordering across owners.
- The parent cookie API retains the seed and both response cookies as
  HttpOnly; `document.cookie` remains empty. No cookie matcher, jar, or raw
  cookie header crosses IPC, and the content process does not retry network
  requests locally.
- If this process regression exposes a dropped cookie update, fix the smallest
  parent-side propagation path. Do not grant cookie authority to a child.

## Tradeoff

Fetch rejects the response for script while the browser network layer still
processes the credentialed response cookie. This slice verifies that distinction
through both page and DedicatedWorker broker routes. It deliberately uses
simple GETs, so it does not re-test XHR preflight behavior or claim full
Fetch/CORS, Web IDL/WPT, platform, or remote-CI conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-860.md`
- `docs/plan/reviews/native-engine-browser-860-01.md`

## Verification

- Add one bounded two-origin process regression covering page Fetch,
  DedicatedWorker Fetch, ordered parent response-cookie acceptance, and
  HttpOnly filtering.
- Run scoped `cargo check` for `glass-browser` and the affected integration
  target before running the exact test, using shared
  `/home/ubuntu/work/glass/target`. Do not run workspace-wide tests.
- Run the exact process-backed regression. If a run exposes a fixture or
  contract failure, correct it and rerun this same regression; do not broaden
  validation to unrelated tests.
- Run formatting, `git diff --check`, and all four maintainer documentation
  gates after final docs edits.
- Record local-only evidence; no push, remote CI, or browser-completion claim.

## Results

- Page and DedicatedWorker Fetch now reject parent-brokered network failures
  with actual `TypeError` instances. Timeout failures retain their existing
  `TimeoutError` behavior. Cookie storage and matching remain parent-owned.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p
  glass-browser --features native-engine --test native_engine --locked
  --quiet` passed with existing dead-code warnings.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p
  glass-browser --features native-engine --test native_engine --locked
  --quiet native_content_process_fetch_cors_errors_keep_parent_cookies --
  --exact` passed (1 passed; 924 filtered; 20.20 seconds).
- The process fixture verifies page and DedicatedWorker CORS failures are
  rejected as `TypeError` instances. The page's HttpOnly response cookie is
  sent on the Worker request; both response cookies and the HttpOnly seed are
  sent on the later authorized page request. The parent cookie API retains all
  three as HttpOnly, and `document.cookie` remains empty.
- The first test run exposed a fixture mistake: the final Fetch Promise was not
  awaited, so the test evaluator returned an empty object instead of its body.
  The fixture now awaits the response body, and the exact regression passes.
- No cookie jar, matcher, or raw Cookie header crosses IPC; the content
  process does not retry requests. This is local focused evidence only, not
  complete Fetch/CORS or WPT conformance, cross-platform certification, or
  remote CI. Issue #40 remains open.
