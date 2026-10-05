---
id: native-engine-browser-857
scope: glass-browser/native-engine/xhr-credentialed-preflight-cache-isolation
status: in-progress
depends-on: [native-engine-browser-856]
---

# Glass native-engine browser slice 857: isolate credentialed XHR preflight cache

## Objective

Add process-backed coverage proving a successful noncredentialed XHR preflight
cannot authorize a later credentialed request to skip its own preflight. Keep
request-cookie selection and response-cookie acceptance parent-owned.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#xmlhttprequest-credentials`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-854.md` — XHR preflight behavior.
- `docs/plan/tasks/native-engine-browser-855.md` — wildcard-origin denial.
- `docs/plan/tasks/native-engine-browser-856.md` — method/header denial.
- `docs/plan/tasks/native-engine-browser-843.md` — parent broker and
  fail-closed contract.
- `docs/architecture/native-engine.md` — parent CORS/network owner.
- [Fetch Standard: CORS-preflight cache](https://fetch.spec.whatwg.org/#cors-preflight-cache)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- On one page-to-API URL, a default `withCredentials == false` XHR first
  performs a cross-origin preflight with a long `Access-Control-Max-Age`, then
  sends its actual POST without cookies. Its wildcard-authorized response
  cannot set a parent cookie.
- A subsequent XHR to the exact same URL/method/header set with
  `withCredentials == true` must perform a fresh OPTIONS request. That
  preflight carries no Cookie and must authorize the exact origin, method,
  headers, and credentials before any actual POST can carry a parent-matched
  cookie.
- The exact credentialed cache entry may then be reused by a later `include`
  XHR without another OPTIONS. Verify the actual request carries the accepted
  HttpOnly response cookie through the parent and `document.cookie` remains
  filtered.
- The API fixture fails if the include request skips its required preflight,
  if an unexpected preflight occurs after successful include authorization,
  or if an actual request carries the wrong cookies.
- Cookie matching, `Set-Cookie` acceptance, and jar ownership remain
  exclusively parent-owned. No cookie jar or raw cookie headers cross IPC, and
  no direct child HTTP(S) retry is permitted.
- This is focused XHR cache coverage, not complete CORS, XHR/Web IDL or WPT
  conformance, cross-platform certification, or remote CI.

## Tradeoff

The current exact credentials-mode cache key is conservative: it prevents an
`omit` entry from authorizing `include`, but can also trigger an extra OPTIONS
request when moving from `include` to a less-privileged mode even where Fetch's
cache permits reuse. This slice protects the credential boundary without
changing the cache policy or claiming complete cache conformance.

## Path

- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-857.md`
- `docs/plan/reviews/native-engine-browser-857-01.md`

## Verification

- Implement one bounded process regression with the same cross-origin URL,
  method, and headers across default, credentialed, and repeated credentialed
  XHR. The fixture must observe OPTIONS/POST order and cookie headers.
- Run scoped `cargo check` for `glass-browser` and the affected integration
  target before the exact test. Use the shared
  `/home/ubuntu/work/glass/target`; do not run workspace-wide tests.
- Run the exact process-backed regression once; do not repeat adjacent tests
  unless production request or owner code changes.
- Run formatting, `git diff --check`, and all four maintainer documentation
  gates after final docs edits.
- Directly review cache isolation, actual-request ordering, and parent cookie
  ownership; no independent agent review is used.
- Record local-only evidence. No remote CI or broad conformance claim is
  implied.
