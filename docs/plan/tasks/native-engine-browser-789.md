---
id: native-engine-browser-789
scope: glass-browser/dedicated-worker-message-callback-errors
status: in-progress
depends-on: [native-engine-browser-788]
---

# Glass native-engine browser slice 789: dedicated-worker message callback errors

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [DOM event-listener invocation algorithm](https://dom.spec.whatwg.org/#concept-event-listener-inner-invoke)
  reports a thrown callback exception for that callback's realm global, then
  continues through the listener list unless immediate propagation was stopped.
- The [HTML exception-reporting algorithm](https://html.spec.whatwg.org/multipage/webappapis.html#report-an-exception)
  fires a cancelable `ErrorEvent` at the global. If it is not handled, a
  dedicated-worker error is queued to its owning `Worker`.
- The [worker runtime-error rule](https://html.spec.whatwg.org/multipage/workers.html#runtime-script-errors)
  applies to uncaught runtime exceptions in worker scripts.
- Slices [787](native-engine-browser-787.md) and
  [788](native-engine-browser-788.md) define the bounded worker-global reporter,
  legacy `onerror` invocation, exact-`true` cancellation, owner forwarding, and
  worker-survival behavior for classic and module startup.

## Objective

Report exceptions thrown by callbacks during dedicated-worker `message` event
delivery at the worker global, continue the remaining callback sequence, and
forward each uncanceled error to the owning page `Worker` in observable command
order.

## Contract

- Apply only to `message` delivery in classic and module dedicated workers
  routed through the dedicated `WorkerGlobalScope` message dispatcher.
- When `onmessage` or an individual registered `message` listener throws a
  JavaScript value, report that value immediately through the existing
  `__glassReportWorkerScriptError` path before invoking the next listener.
- Preserve the current `onmessage`-before-registered-listeners order and the
  existing exact-`false` outer `MessageEvent` cancellation behavior. A callback
  exception does not itself cancel that message event or suppress later
  listeners.
- For every callback exception, invoke worker-global `onerror` with the legacy
  five arguments, then registered worker-global `error` listeners. Only exact
  `true` from worker-global `onerror` cancels forwarding; error listeners still
  run and see the final `defaultPrevented` state.
- If uncanceled, enqueue the owning `Worker`'s `ErrorEvent` at the point of the
  report, preserving its order relative to `postMessage()` commands emitted by
  callbacks and by worker-global error handlers. The owner event retains the
  worker URL/message, zero line and column, and null `error` value.
- Exceptions thrown while the worker global is already reporting an error do
  not recursively start another report. Existing reporter behavior governs
  this case.
- The worker remains usable for a subsequent message after a callback error.
- Shared workers, service workers, MessagePort events, non-message worker
  EventTargets, promise rejections, and scheduling semantics are unchanged.

## Boundaries and tradeoffs

- A bounded internal worker-error command is preferred over collecting errors
  after dispatch: it preserves causal command order across callback-posted
  messages and error forwarding without changing the worker task scheduler.
- This slice does not claim complete DOM `EventTarget`/HTML event-handler or
  Web Platform Test conformance. It does not alter error-source locations,
  console reporting, message-port callback reporting, asynchronous rejection
  tracking, or specialized resource event targets.
- Native-only production, process/sandbox promotion, cross-platform CI, and
  issue #40 completion gates remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-789.md`

## Verification

Pending implementation. The focused regression must cover classic and module
dedicated workers, throwing `onmessage` and registered listeners, multiple
callback errors, global `onerror`/error-listener order and exact-`true`
cancellation, owner forwarding/order, later listeners, and worker survival.
