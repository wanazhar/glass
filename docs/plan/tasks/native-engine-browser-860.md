---
id: native-engine-browser-860
scope: glass-browser/native-engine/fetch-cors-error-cookie-ownership
status: in-progress
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

- `crates/glass-browser/tests/native_engine.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs` (only if
  the regression exposes a parent-side gap)
- `crates/glass-browser/src/browser/native_engine/content_process.rs` (only if
  Fetch-route cookie propagation needs correction)
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
- Run the exact process-backed regression once; repeat only if production
  request or cookie-owner code changes.
- Run formatting, `git diff --check`, and all four maintainer documentation
  gates after final docs edits.
- Record local-only evidence; no push, remote CI, or browser-completion claim.

## Results

- In progress; the process regression has not yet been implemented or run.
