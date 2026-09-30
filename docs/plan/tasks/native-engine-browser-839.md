id: native-engine-browser-839
scope: glass-browser/native-engine/service-worker-navigation-preload-fetch
status: pending
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
- Start the preload concurrently with FetchEvent dispatch. Navigation
  cancellation aborts it. It must not block dispatch while waiting for the
  network response or allow a late response to mutate a committed/cancelled
  navigation.
- `FetchEvent.preloadResponse` resolves to an immutable, readable native
  `Response` on success, rejects with `TypeError` on a network error, and
  resolves to `undefined` when the algorithm does not start a preload.
- Preserve response status, URL, headers, redirect state, and bounded body
  bytes without consuming the worker-visible body. When the fetch handler does
  not call `respondWith`, follow the Service Workers fetch algorithm without
  issuing a duplicate request when the preload result is reusable.
- Tests assert the actual request method, URL, configured header, number of
  network requests, overlap with FetchEvent dispatch, FetchEvent response
  semantics, network failure, and cancellation. Include non-GET, disabled,
  absent-listener, and enabled-listener controls.
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
