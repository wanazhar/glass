---
id: native-engine-browser-862
scope: glass-browser/native-engine/service-worker-fetch-cors-error-cookies
status: in-progress
depends-on: [native-engine-browser-861]
---

# Glass native-engine browser slice 862: ServiceWorker Fetch CORS-error cookies

## Objective

Verify that a CORS-failed actual response to a ServiceWorker Fetch issued
while intercepting a captured navigation still updates the parent-owned cookie
jar, and that a later authorized ServiceWorker request uses the accepted
cookie. Preserve the parent as the sole cookie authority.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/architecture/native-engine.md#cookie-authority-and-process-boundary`
- `docs/plan/tasks/native-engine-browser-843.md` — parent broker and
  captured-load ownership audit.
- `docs/plan/tasks/native-engine-browser-860.md` — page and DedicatedWorker
  Fetch CORS-error response-cookie evidence.
- `docs/plan/tasks/native-engine-browser-861.md` — browser-owned SharedWorker
  CORS-error response-cookie evidence.
- [Fetch Standard: CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Network-owner crosswalk

| Request class | Owner and existing evidence | Slice 862 treatment |
|---|---|---|
| Page and DedicatedWorker Fetch | Exact-owner content-process requests use the parent broker; Slice 860 verifies the actual CORS-error response cookie and `TypeError`. | No change. |
| Browser-owned SharedWorker Fetch | Coordinator uses the parent loader and shared context cookie jar; Slice 861 verifies CORS-error cookie acceptance and reuse. | No change. |
| ServiceWorker FetchEvent Fetch | `NativeServiceWorkerRegistry` routes process-backed Fetch commands through `NativeContentFetchBroker`; captured navigation work carries an exact-owner marker and uses `fetch_for_captured_load`. Existing lifecycle/navigation tests cover successful cookie flow, but not an actual-response CORS failure that sets a cookie. | Verify one captured-navigation route end to end with real loopback HTTP origins. |

The crosswalk distinguishes the browser-owned SharedWorker loader from
ServiceWorker registry requests brokered on behalf of a content process. It
does not imply that autonomous background ServiceWorker network APIs are
available or parent-brokered.

## Contract

- Use separate loopback page and API origins and simple credentialed GETs. The
  failing response must be the actual response, not a preflight.
- Register and activate a ServiceWorker from a process-backed browser runtime.
  During an intercepted navigation, its FetchEvent makes a credentialed
  cross-origin Fetch. The API returns wildcard `Access-Control-Allow-Origin`,
  `Access-Control-Allow-Credentials: true`, and a distinct HttpOnly
  `Set-Cookie`.
- The ServiceWorker observes a rejected Fetch with an actual `TypeError` and
  cannot read the response status or body. It returns a controlled navigation
  response proving that rejection was handled.
- A later authorized ServiceWorker Fetch and a page Fetch send the newly
  accepted cookie. The parent cookie API retains the seed, ServiceWorker
  entry, and actual-response cookies as HttpOnly; `document.cookie` excludes
  them.
- The HTTP fixture asserts cookie selection, origin, request count, and absence
  of preflight. Preserve captured-load validation against the exact active or
  in-flight context, frame, generation, and URL.
- Cookie matching, CORS response-cookie acceptance, and persistence stay in
  the browser parent. Do not add a child cookie jar, cookie profile, or raw
  `Cookie`/`Set-Cookie` IPC. Do not add child-side HTTP(S) fallback or retry.
- If the regression exposes a defect, change only the ServiceWorker parent
  response propagation needed to meet this route's contract.

## Tradeoff

The captured-navigation ServiceWorker Fetch uses a narrowly authorized
parent-broker path, while browser-owned SharedWorker requests use a separate
parent loader route. This slice tests their remaining distinct CORS-cookie
boundary without broadening the cookie authority or claiming background
ServiceWorker support. Its regression adds loopback integration-test time but
no production work when the existing contract is correct.

## Path

- `crates/glass-browser/src/browser/native_engine/service_worker.rs` (only if
  the regression exposes a ServiceWorker broker propagation defect)
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs` (only if
  parent response-cookie acceptance is missing on this route)
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-862.md`
- `docs/plan/reviews/native-engine-browser-862-01.md`

## Verification

- Add one bounded two-origin, process-backed regression for a captured-load
  ServiceWorker FetchEvent, a credentialed actual-response CORS failure, the
  resulting HttpOnly cookie in the parent jar, and later authorized reuse.
- Run one scoped `cargo check` for `glass-browser` and the integration target
  before running the exact regression. Reuse
  `/home/ubuntu/work/glass/target`; do not create a worktree-local target or
  run workspace-wide tests.
- Run the exact regression only. If fixture timing or contract behavior fails,
  correct the fixture/implementation as one batch and rerun this same test.
- Run formatting, `git diff --check`, and all four maintainer documentation
  gates after the final documentation edits. Reuse existing inventory
  binaries via their explicit shared-target paths.
- Commit locally using the configured user Git identity. Do not push code or
  claim remote CI, cross-platform certification, broad Fetch/CORS or WPT
  conformance. Keep issue #40 open.

## Results

Implementation and focused verification are pending.
