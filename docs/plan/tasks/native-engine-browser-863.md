---
id: native-engine-browser-863
scope: glass-browser/native-engine/service-worker-fetch-cors-error-cookies
status: in-progress
depends-on: [native-engine-browser-862]
---

# Glass native-engine browser slice 863: ordinary ServiceWorker broker CORS cookies

## Objective

Verify that a credentialed Fetch emitted by a ServiceWorker handling an
explicit controlled-page Fetch uses the ordinary parent broker (not the
captured-load route), and that an actual response rejected by CORS still
updates only the parent's cookie jar for later authorized requests.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/architecture/native-engine.md#cookie-authority-and-process-boundary`
- `docs/plan/tasks/native-engine-browser-843.md` — parent broker and
  captured-load ownership audit.
- `docs/plan/tasks/native-engine-browser-862.md` — captured-navigation
  ServiceWorker CORS-error cookie evidence.
- [Fetch Standard: CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Network-owner crosswalk

| Request class | Owner and evidence | Slice 863 treatment |
|---|---|---|
| Captured-navigation ServiceWorker FetchEvent Fetch | Exact-owner captured-load broker uses `fetch_for_captured_load`; Slice 862 verifies an actual-response CORS failure and parent cookie reuse. | Preserve as baseline; no new captured-load request. |
| Explicit controlled-page FetchEvent Fetch | ServiceWorker registry uses the ordinary owner-checked parent broker when settling the page's explicit FetchEvent. Existing process tests cover successful CORS/cookie behavior, but not an actual-response CORS failure that sets a cookie on this ordinary broker branch. | Extend the process regression with a distinct CORS failure and cookie. |
| Page and DedicatedWorker Fetch | Parent-brokered CORS-error cookie behavior is covered by Slice 860. | No change. |
| Browser-owned SharedWorker Fetch | Parent coordinator CORS-error cookie behavior is covered by Slice 861. | No change. |

This slice isolates `NativeContentFetchBroker::fetch` from the captured-load
variant. It does not imply that autonomous background ServiceWorker requests
or unimplemented ServiceWorker network APIs are available.

## Contract

- Reuse separate loopback page and API origins. The ServiceWorker must
  intercept a page-issued explicit Fetch and make its own credentialed
  cross-origin simple GET through the ordinary broker path.
- The API sends an actual response with wildcard
  `Access-Control-Allow-Origin`, `Access-Control-Allow-Credentials: true`, and
  a distinct HttpOnly `Set-Cookie`; no preflight is permitted.
- The ServiceWorker observes an actual `TypeError` with no response status or
  body, and the page receives only the worker's bounded handled result.
- A subsequent authorized ServiceWorker request and a page request carry the
  accepted response cookie. Parent cookie API visibility confirms that it is
  HttpOnly; `document.cookie` excludes it.
- Keep the captured-load and ordinary broker requests distinct in the same
  bounded process-backed regression. Assert request order, origin, method,
  cookies, no preflight, and no unexpected API calls.
- Cookie matching, response-cookie acceptance, and jar ownership remain in the
  browser parent. Do not add child cookie state, profile data, raw cookie
  headers to IPC, or direct child HTTP(S) fallback.
- If the regression reveals a defect, fix only the ordinary ServiceWorker
  broker response propagation required for this contract.

## Tradeoff

This adds a second CORS-failed actual response to the existing two-origin
ServiceWorker fixture so one process test exercises both parent broker entry
points. That slightly increases fixture complexity and runtime, but avoids a
duplicate browser startup and directly detects behavioral drift between
captured and ordinary owner-checked requests. It does not add production
behavior unless a defect is found.

## Path

- `crates/glass-browser/src/browser/native_engine/service_worker.rs` (only if
  the ordinary-broker regression exposes a propagation defect)
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs` (only if
  parent response-cookie acceptance is missing on this route)
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-863.md`
- `docs/plan/reviews/native-engine-browser-863-01.md`

## Verification

- Extend the focused two-origin process regression to exercise both the
  captured-load route and a controlled-page FetchEvent dispatched by an
  explicit page Fetch, verifying a distinct actual-response CORS failure and
  HttpOnly cookie for each.
- Run one scoped `cargo check` for `glass-browser` and the integration target
  before the exact test. Reuse `/home/ubuntu/work/glass/target`; do not create
  a worktree-local target or run workspace-wide tests.
- Run only the exact ServiceWorker CORS-cookie regression. Correct any fixture
  or implementation issue in one batch and rerun that exact regression.
- Run formatting, `git diff --check`, and all four maintainer documentation
  gates after final documentation edits. Reuse the existing shared-target
  inventory binaries by explicit path.
- Commit locally with the configured user Git identity. Do not push code or
  claim remote CI, platform certification, or broad Fetch/CORS/WPT
  conformance. Keep issue #40 open.

## Results

Implementation and focused verification are pending.
