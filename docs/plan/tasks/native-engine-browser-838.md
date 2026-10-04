id: native-engine-browser-838
scope: glass-browser/native-engine/service-worker-navigation-preload-manager
status: complete
depends-on: []
---

# Glass native-engine browser slice 838: NavigationPreloadManager state

## Objective

Implement the page-facing `ServiceWorkerRegistration.navigationPreload` manager
and persist its settings as part of the ServiceWorker registration profile.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-837.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [Service Workers: NavigationPreloadManager](https://w3c.github.io/ServiceWorker/#navigationpreloadmanager)
- [Fetch Standard: header values](https://fetch.spec.whatwg.org/#concept-header)

## Contract

- Every page-realm ServiceWorker registration exposes the same
  `NavigationPreloadManager` object for repeated reads of its
  `navigationPreload` property.
- Implement promise-returning `enable()`, `disable()`, `setHeaderValue(value)`,
  and `getState()`. State defaults to disabled with header value `true`.
- `enable()`, `disable()`, and `setHeaderValue()` reject with a
  `DOMException` named `InvalidStateError` when the registration has no active
  worker. `getState()` works without an active worker and returns both
  `enabled` and `headerValue`.
- Convert `setHeaderValue` input using `ByteString` constraints, normalize the
  byte sequence using Fetch's HTTP-whitespace rules, and reject an invalid
  header value with `TypeError`. Validate again at the native command and
  persisted-profile boundaries; do not silently repair malformed IPC or disk
  state.
- Persist enabled and header-value state on the registration profile. Missing
  fields in existing profiles decode to the defaults. Register/update,
  waiting-worker promotion, scoped profile merge, restoration, and journal
  writes preserve the current settings unless an explicit Navigation Preload
  command changes them.
- Keep settings isolated by exact same-origin registration scope. Unknown or
  cross-origin scopes must not read or mutate another registration's state.
- Do not claim request behavior in this slice. The native
  `FetchEvent.preloadResponse` currently resolves to `undefined`; Slice 839 owns
  actual request dispatch, response delivery, and cancellation.
- Socket-free runtime and persistence tests cover manager identity, command
  payloads, state response and error mapping, defaulting old profiles, and
  preserving settings through registration profile mutations. A
  process-backed test covers page API through content-process persistence;
  record it as unverified if the sandbox rejects the test listener before
  engine startup.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-838.md`

## Verification

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --features native-engine --lib --tests --locked -q`
- Focused socket-free NavigationPreloadManager and profile-persistence unit
  tests.
- Process-backed
  `native_content_process_persists_service_worker_cache_across_restart` after
  the scoped check.
- `git diff --check`.

## Progress

- The scoped package check passes with only existing unused legacy HTML-parser
  warnings. The socket-free tests
  `service_worker_navigation_preload_manager_is_stable_and_validates_header_values`
  and `navigation_preload_state_requires_active_worker_and_survives_replacement`
  pass (2 passed, 1,653 filtered).
- The process-backed
  `native_content_process_persists_service_worker_cache_across_restart`
  regression sets manager state through the page API and verifies it after
  profile reload. It passed on the current checkout (1 passed; 906 filtered;
  39.70 seconds).
- `cargo fmt --all -- --check`, documentation coverage and depth gates, and
  `git diff --check` pass. The manager and process-backed persistence contract
  is complete for this slice. Actual network preloading and
  `FetchEvent.preloadResponse` remain assigned to Slice 839.
