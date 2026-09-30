id: native-engine-browser-839
scope: glass-browser/native-engine/service-worker-navigation-preload-fetch
status: in-progress
depends-on: [native-engine-browser-838]
---

# Glass native-engine browser slice 839: Navigation preload Fetch integration

## Objective

Execute enabled ServiceWorker navigation-preload requests through the native
network layer and expose their results through `FetchEvent.preloadResponse`.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-838.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [Service Workers: Handle Fetch](https://w3c.github.io/ServiceWorker/#handle-fetch)
- [Service Workers: `FetchEvent.preloadResponse`](https://w3c.github.io/ServiceWorker/#dom-fetchevent-preloadresponse)
- [Fetch Standard](https://fetch.spec.whatwg.org/)

## Contract

- Start preload only for a matching navigation whose request method is `GET`,
  whose registration is enabled, and whose active ServiceWorker has a
  non-empty `fetch` listener set.
- Clone the navigation request, add exactly the registration's
  `Service-Worker-Navigation-Preload` header value, and bypass ServiceWorker
  interception for that clone. Reuse the native loader's URL, origin,
  credentials, redirect, network-policy, cookie, response-size, and
  cancellation behavior; do not create a second browser/backend path.
- Carry the source Document URL and its effective Referrer-Policy separately
  across the browser/content-process boundary. Apply that policy to the
  initial preload and recompute the `Referer` after redirects, including
  response `Referrer-Policy` updates; never infer policy from an already
  shortened referrer URL. Expose `FetchEvent.request.referrer` in its
  policy-filtered form, including a reduced cross-origin value, while retaining
  the full source URL only in internal request state for network and preload
  calculations. Only the internal navigation token may bypass the public
  Request constructor's same-origin referrer restriction.
- Start the preload concurrently with FetchEvent dispatch. Navigation
  cancellation aborts it. It must not block dispatch while waiting for the
  network response or allow a late response to mutate a committed/cancelled
  navigation. If the handler has already produced its own response while the
  preload is still pending, commit the ServiceWorker response without waiting
  for the unused preload body and abort that pending request.
- The navigation `FetchEvent.request` exposes `mode === "navigate"`. Create
  that internal request without weakening the public `Request` constructor's
  rejection of `new Request(url, { mode: "navigate" })`.
- `FetchEvent.preloadResponse` resolves to an immutable, readable native
  `Response` on success, rejects with `TypeError` on a network error, and
  resolves to `undefined` when the algorithm does not start a preload.
- The `respondWith()` promise alone determines when the FetchEvent response is
  ready. Independent `waitUntil()` promises extend the event lifetime without
  delaying that response or converting their rejection into a navigation
  failure. The `respondWith()` promise itself also extends the event lifetime.
  Lifetime work, including native host commands it emits, must remain owned and
  continue after the response is returned; it must not be dropped to meet the
  response deadline.
- Preserve response status, URL, headers, redirect state, and bounded body
  bytes without consuming the worker-visible body. When the fetch handler does
  not call `respondWith`, follow the Service Workers fetch algorithm without
  issuing a duplicate request when the preload result is reusable.
- Tests assert the actual request method, URL, configured header, number of
  network requests, overlap with FetchEvent dispatch, navigation request mode,
  `Request.referrer`/`referrerPolicy`, source-document `Referer` policy, redirect
  policy updates, FetchEvent response semantics, network failure, and
  cancellation. Also verify a pending unrelated `waitUntil()` does not delay an
  independent response, while its lifetime work remains alive and can settle.
  Include non-GET,
  disabled, absent-listener, and enabled-listener controls.
- Process-backed network assertions are required; a pure command or mocked
  loader test alone does not close this slice.

## Path

- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-839.md`

## Verification

- Focused process-backed integration tests for actual HTTP and JavaScript
  behavior, including cancellation and no duplicate request.
- `cargo check -p glass-browser --features native-engine --lib --tests --locked -q`
- `cargo fmt --all -- --check`
- Relevant Fetch and Service Worker WPT cases, with selected cases and
  deviations recorded.
- `git diff --check`.

## Current Evidence

- `cargo check -p glass-browser --features native-engine --lib --tests --locked -q` passes on the current revision.
- `cargo test -p glass-browser --lib --features native-engine --locked navigation_preload -- --quiet` passes (six socket-free tests, including the immediate-response runtime regression).
- `cargo test -p glass-browser --lib --features native-engine --locked service_worker_fetch_response_does_not_wait_for_wait_until_lifetime -- --quiet` passes (one socket-free regression).
- `cargo fmt --all -- --check`, release-documentation truth, documentation depth, TUI shortcut inventory, and `git diff --check` pass.
- The response test proves headers are immutable and both the original and
  cloned response bodies remain readable. It also verifies the policy-reduced
  cross-origin `Request.referrer` and that public `Request` construction still
  rejects navigation mode.
- A socket-free runtime regression returns an independent response while a
  JavaScript-only `waitUntil()` promise remains pending and retained by the
  worker. This does not prove that host commands emitted by lifetime work
  continue asynchronously: the current Rust settlement loop still processes
  FetchEvent host commands before returning the response.
- The new process-backed independent-response regression compiles as part of
  `cargo check --tests` and the integration-test target. Running it fails at
  `TcpListener::bind("127.0.0.1:0")` with `PermissionDenied` before engine
  startup, so its behavioral assertion is not yet verified.
- Process-backed HTTP request/header/no-duplicate, navigation-cancellation,
  source-policy/redirect, worker-visible `Request.referrer`, and independent-
  response behavior still need execution evidence. WPT, remote CI, and
  cross-platform validation remain open.
