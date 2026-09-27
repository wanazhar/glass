---
id: native-engine-browser-797
scope: glass-browser/service-worker-install-activate-callback-errors
status: completed
depends-on: [native-engine-browser-796]
---

# Glass native-engine browser slice 797: Service Worker lifecycle callback errors

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [DOM event-listener invocation algorithm](https://dom.spec.whatwg.org/#concept-event-listener-inner-invoke)
  reports exceptions at the callback realm's global and continues later event
  listeners unless immediate propagation is stopped.
- The [Service Worker specification](https://w3c.github.io/ServiceWorker/)
  dispatches `install` and `activate` events at ServiceWorkerGlobalScope and
  separately waits for their asynchronous lifetime extensions. Rejection of
  an install event's lifetime promises still fails installation.
- The native `__glassDispatchServiceWorkerLifecycle` currently catches each
  registered callback exception and discards it, then returns a `Promise.all`
  over the existing `waitUntil()` promises.
- Slices [795](native-engine-browser-795.md) and
  [796](native-engine-browser-796.md) establish global-only Service Worker
  exception reporting for global `message` and `fetch` callbacks.

## Objective

Report synchronous exceptions from registered Service Worker `install` and
`activate` listeners at ServiceWorkerGlobalScope and continue dispatch, without
changing lifecycle success/failure decisions made by `waitUntil()` promises.

## Contract

- Apply only to registered callbacks dispatched for Service Worker global
  `install` and `activate` events. Preserve the current listener snapshot and
  registration order. Event-handler attributes such as `oninstall` and
  `onactivate`, which the current lifecycle dispatcher does not model, are a
  separate Web IDL/event-handler scope.
- Report each synchronously thrown JavaScript value through the existing
  worker-global `__glassReportWorkerScriptError` path before invoking the next
  lifecycle listener. Exact-`true` global `onerror` cancellation affects only
  the cancelable ErrorEvent; registered global `error` listeners still run and
  observe the final `defaultPrevented` state.
- Do not forward a Service Worker lifecycle callback exception to a page
  `Worker`, `ServiceWorker`, client, registration, or
  `ServiceWorkerContainer` object. Do not enqueue the dedicated-worker
  `WorkerScriptError` command.
- A synchronous callback throw alone does not reject the lifecycle dispatch,
  skip later listeners, deactivate the worker, or suppress a fulfilled
  `waitUntil()` action from a later listener.
- Preserve `waitUntil()` lifetime and settlement: fulfilled promises still
  permit the requested lifecycle transition; a rejected install lifetime
  promise still fails installation. Do not reinterpret a callback throw as a
  rejected lifetime promise.
- Add a process-backed HTTP(S) registration regression with a throwing
  `install` listener followed by a listener that calls `waitUntil(skipWaiting())`,
  and a throwing `activate` listener followed by a listener that calls
  `waitUntil(clients.claim())`. Capture the worker-global report order/state,
  verify both later listeners run, registration reaches `activated`, the page
  becomes controlled, and no client error event is delivered.
- DedicatedWorker, SharedWorker, Service Worker message/fetch/MessagePort,
  BroadcastChannel, rejected `waitUntil()` behavior, and generic scheduler
  behavior must remain unchanged.

## Boundaries and tradeoffs

- Do not add event-handler attributes, change event object identity/targets,
  alter install/activate state transitions, change `waitUntil()` timeout or
  rejection processing, registration/update persistence, lifecycle task-source
  behavior, startup-error reporting, promise-rejection reporting, console
  reporting, or error source locations.
- Do not alter client routing, resource loading, security policy, shutdown, or
  worker recovery. This slice is not complete Service Worker lifecycle,
  `ExtendableEvent`, HTML error-reporting, or Web Platform Test conformance.
- Remote CI, cross-platform certification, and issue #40 production/release
  gates remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-797.md`

## Implementation and verification

Contract checkpoint: `423adf1c`; implementation and process-backed regression:
`378e6222`.

The lifecycle dispatcher now catches each registered callback's synchronous
exception independently and reports it via
`reportWorkerCallbackException(error, false)`. It continues the callback
snapshot and leaves its original `Promise.all(waitUntilPromises)` settlement
unchanged: a fulfilled wait still permits the transition, while a rejected
wait remains a lifecycle rejection. No dedicated regression for rejected
`waitUntil()` was added in this slice; the settlement implementation itself
was not modified.

The process-backed HTTP regression
`native_content_process_service_worker_lifecycle_callback_errors_continue`
registers an install listener that throws an `Error` handled by exact-`true`
global `onerror`, followed by a listener using `waitUntil(skipWaiting())`. Its
activate listener throws an uncanceled primitive, followed by a listener using
`waitUntil(clients.claim())`. The page receives snapshots proving report order,
global ErrorEvent state, continued dispatch, activated state, controller
ownership, and no page error event.

Passed locally:

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_service_worker_lifecycle_callback_errors_continue -- --exact` (1 passed).
- Direct regressions from `target/debug/deps/native_engine-d4489efcb3153c24`: `native_content_process_service_worker_message_callback_errors_stay_global` and `native_content_process_service_worker_fetch_callback_errors_continue_and_fallback` (1 passed each).
- `cargo fmt --all -- --check` and `git diff --check`.

The integration target emits existing native-DOM dead-code warnings. Remote CI
and complete Service Worker lifecycle, rejected-lifetime regression coverage,
cross-platform certification, and WPT conformance remain issue #40 gates.
