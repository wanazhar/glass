---
id: native-engine-browser-861
scope: glass-browser/native-engine/shared-worker-fetch-cors-error-cookies
status: complete
depends-on: [native-engine-browser-860]
---

# Glass native-engine browser slice 861: SharedWorker Fetch CORS-error cookies

## Objective

Extend the parent-cookie ownership audit to the browser-owned SharedWorker
Fetch route. Verify that a credentialed actual response rejected by CORS still
updates the parent's cookie jar and that later authorized requests use the
cookie.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/architecture/native-engine.md#cookie-authority-and-process-boundary`
- `docs/plan/tasks/native-engine-browser-843.md` — parent broker and
  browser-owned SharedWorker ownership evidence.
- `docs/plan/tasks/native-engine-browser-860.md` — page and DedicatedWorker
  Fetch CORS-error cookie evidence.
- [Fetch Standard: CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Network-owner crosswalk

| Request class | Current owner/evidence | Slice 861 treatment |
|---|---|---|
| Page and DedicatedWorker Fetch | Owner-checked parent broker; Slice 860 verifies CORS-error cookies and actual `TypeError` rejection. | No change. |
| Browser-owned SharedWorker Fetch | `NativeWorkerRegistry::new_with_parent_network_authority` runs the coordinator with the parent loader and shared browser cookie jar. Existing runtime tests cover request credentials, redirects, response-cookie rotation, profile fan-out, and persistence. | The exact Slice 861 regression now covers the CORS-error response-cookie sequence. |
| ServiceWorker Fetch | ServiceWorker registry and captured-load parent-broker routes are covered by Slice 843 follow-ups and dedicated process tests. | No change; keep ServiceWorker error-response evidence separate. |

This crosswalk does not claim that every HTTP(S) request class or every
SharedWorker lifecycle route has complete parent-only ownership or conformance.
Uncovered child requests must continue to fail closed without local HTTP(S)
fallback.

## Contract

- Use distinct loopback page and API origins and simple GET requests. The API
  response that fails must be the actual response, not a preflight.
- The page and SharedWorker entry receive a parent-selected HttpOnly seed
  cookie. The SharedWorker makes a credentialed Fetch to the API.
- The API returns wildcard `Access-Control-Allow-Origin`,
  `Access-Control-Allow-Credentials: true`, and a distinct HttpOnly
  `Set-Cookie`. The SharedWorker Fetch promise rejects with an actual
  `TypeError`; script receives no response status or body.
- A subsequent authorized SharedWorker request and page request carry the
  response cookie. The parent cookie API retains the seed and response cookie
  as HttpOnly, while `document.cookie` stays empty.
- Keep cookie matching, response-cookie acceptance, and persistence in the
  browser parent. Do not send a cookie jar or raw Cookie/Set-Cookie header to
  the content process. Do not introduce child-side HTTP(S) retries.
- If the regression exposes a dropped update, fix only the parent response
  propagation path required by this route.

## Tradeoff

The parent-owned SharedWorker coordinator is a distinct request route from the
content-process broker used by page and DedicatedWorker Fetch. This slice
checks that route's Fetch/CORS side effects without combining it with the
ServiceWorker lifecycle or expanding the cookie authority boundary. It does
not claim complete SharedWorker, Fetch/CORS, Web IDL/WPT, platform, or remote-CI
conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs` (only if the
  regression finds a SharedWorker response-dispatch defect)
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs` (only if
  parent response-cookie commit behavior is missing on this path)
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-861.md`
- `docs/plan/reviews/native-engine-browser-861-01.md`

## Verification

- Add one bounded two-origin regression for a browser-owned SharedWorker,
  CORS-failed credentialed Fetch, parent cookie acceptance, and later
  authorized requests.
- Run scoped `cargo check` for `glass-browser` and the integration target
  before the exact test. Use the shared `/home/ubuntu/work/glass/target`; do
  not create a worktree-local target tree or run workspace-wide tests.
- Run the exact process-backed regression. If a run exposes a fixture or
  contract failure, correct and rerun this same regression; do not broaden
  validation to unrelated tests.
- Run formatting, `git diff --check`, and all four maintainer documentation
  gates after final docs edits. Reuse existing inventory binaries by passing
  their explicit shared-target paths.
- Record local-only evidence. Do not push code or claim remote CI,
  cross-platform, or broad conformance.

## Results

- The browser-owned SharedWorker path required no runtime fix. The parent
  loader accepts the actual-response cookie before returning its script-visible
  CORS error; its shared cookie jar serves the later SharedWorker and page
  requests.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p
  glass-browser --features native-engine --test native_engine --locked
  --quiet` passed with existing dead-code warnings.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p
  glass-browser --features native-engine --test native_engine --locked
  --quiet native_runtime_shared_worker_fetch_cors_errors_keep_parent_cookies
  -- --exact` passed (1 passed; 925 filtered; 21.85 seconds).
- The loopback fixture verified that the SharedWorker sent the parent-selected
  seed and entry cookies on the credentialed request. The actual response
  used wildcard `Access-Control-Allow-Origin`, credentials, and an HttpOnly
  `Set-Cookie`; Fetch rejected with an actual `TypeError` instance. The next
  authorized SharedWorker request and page request both carried the new cookie.
  The parent cookie API retained all three cookies as HttpOnly, while
  `document.cookie` remained empty. The fixture observed no preflight or extra
  request.
- Formatting and maintainer documentation gate evidence are recorded in the
  Slice 861 review. This is local focused evidence only; it does not certify
  complete SharedWorker, Fetch/CORS or WPT conformance, other platforms,
  independent review, or remote CI. Issue #40 remains open.
