---
id: native-engine-browser-796
scope: glass-browser/service-worker-fetch-callback-errors
status: in_progress
depends-on: [native-engine-browser-795]
---

# Glass native-engine browser slice 796: Service Worker fetch callback errors

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [DOM event-listener invocation algorithm](https://dom.spec.whatwg.org/#concept-event-listener-inner-invoke)
  reports an exception at the callback realm's global and continues to later
  event listeners unless immediate propagation is stopped.
- The [Service Worker Handle Fetch algorithm](https://w3c.github.io/ServiceWorker/#handle-fetch)
  dispatches a `FetchEvent`, then distinguishes a supplied `respondWith()`
  response from a fetch event that did not respond. Without a response, the
  fetch continues through its ordinary fallback path. A callback throw is not
  itself a rejected `respondWith()` promise.
- The native `serviceWorkerDispatchFetch` currently wraps `onfetch` and all
  fetch listeners in one `try`/`catch`; a synchronous callback exception
  rejects the dispatch promise, skipping remaining listeners and conflating
  callback failure with fetch-response failure.
- Slice [795](native-engine-browser-795.md) establishes global-only reporting
  for Service Worker global message callbacks. Dedicated-worker owner
  forwarding must not be reused for a Service Worker fetch event.

## Objective

Report synchronous exceptions thrown by a Service Worker `onfetch` handler or
registered `fetch` listener at ServiceWorkerGlobalScope, continue dispatch,
and preserve the FetchEvent's existing response/fallback behavior.

## Contract

- Apply only to synchronous exceptions thrown while invoking Service Worker
  global `onfetch` and registered `fetch` callbacks. Report each thrown value
  through the existing worker-global `__glassReportWorkerScriptError` path
  before invoking the next fetch callback.
- Preserve `onfetch`-before-registered-listener order and current listener
  snapshot/iteration behavior. Exact-`true` global `onerror` cancellation
  affects only the global ErrorEvent; it does not cancel fetch dispatch or
  listener continuation. Registered global `error` listeners still run and
  observe the final `defaultPrevented` state.
- Do not forward a Service Worker fetch callback exception to a page `Worker`,
  `ServiceWorker`, client, registration, or `ServiceWorkerContainer` object,
  and do not enqueue a dedicated-worker `WorkerScriptError` command.
- A callback exception must not erase a `respondWith()` call that already
  occurred or prevent a later fetch listener from supplying a response. When
  dispatch ends without `respondWith()`, preserve the ordinary network
  fallback; do not turn the callback exception into a rejected fetch-response
  promise.
- Promise rejection from a response passed to `respondWith()` remains the
  existing response failure behavior. Do not reinterpret it as a synchronous
  callback exception.
- Add process-backed HTTP(S) coverage for: an `onfetch` Error followed by a
  registered listener that supplies a response; an uncanceled primitive from
  one registered listener followed by a listener that supplies a response; a
  thrown callback with no supplied response falling through to the network;
  global report order and cancellation state; and a subsequent healthy
  controlled fetch. Verify the Service Worker remains active and no client
  error event is delivered.
- DedicatedWorker, SharedWorker, Service Worker `message`/MessagePort,
  BroadcastChannel, response-promise rejection, and generic task-scheduling
  behavior must remain unchanged.

## Boundaries and tradeoffs

- Do not change `respondWith()` validation/locking, response Promise
  fulfillment or rejection, `waitUntil()` lifetime, fetch request/response
  conversion, cache/network policy, preload/routing, event cancellation,
  Service Worker lifecycle, promise-rejection reporting, console reporting,
  error source locations, or generic worker dispatch.
- Do not alter request routing, client ownership, task-source selection,
  security policy, or retry behavior. A callback error by itself must not
  silently authorize a response or suppress a normal network request.
- The focused regression is not complete Service Worker Fetch, DOM event
  dispatch, HTML error-reporting, or Web Platform Test conformance. Remote CI,
  cross-platform certification, and issue #40 production/release gates remain
  open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-796.md`

## Implementation and verification

Commit this contract before implementation. Record the exact process-backed
regression and scoped local verification commands here after implementation.
Remote CI and complete Service Worker Fetch/WPT conformance remain issue #40
gates.
