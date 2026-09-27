---
id: native-engine-browser-793
scope: glass-browser/dedicated-worker-message-port-callback-errors
status: in_progress
depends-on: [native-engine-browser-792]
---

# Glass native-engine browser slice 793: DedicatedWorker MessagePort callback errors

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [DOM event-listener invocation algorithm](https://dom.spec.whatwg.org/#concept-event-listener-inner-invoke)
  reports a callback exception for the callback realm's global, then continues
  through the listener list unless immediate propagation was stopped.
- The [HTML exception-reporting algorithm](https://html.spec.whatwg.org/multipage/webappapis.html#report-an-exception)
  first fires a cancelable `ErrorEvent` at that global. If the global is a
  `DedicatedWorkerGlobalScope` and the error is not handled, it queues an
  `ErrorEvent` at the associated page `Worker`.
- Slice [789](native-engine-browser-789.md) implements that report/forward path
  for dedicated-worker global `message` callbacks. Slice
  [792](native-engine-browser-792.md) implements MessagePort callback
  reporting only for SharedWorker globals. The common MessagePort dispatcher
  still has no dedicated-worker reporting hook.

## Objective

Report exceptions from a MessagePort `onmessage` handler or registered
`message` listener in a dedicated worker at its worker global. Honor global
`onerror` cancellation, forward uncanceled errors to the owning page `Worker`,
and continue port delivery and later callbacks.

## Contract

- Apply only to native MessagePort message dispatch in a dedicated-worker
  realm. Do not infer the realm from the page-owned endpoint. The transferred
  worker endpoint must report to the dedicated worker global.
- Use the existing dedicated-worker `__glassReportWorkerScriptError` path for
  each thrown JavaScript value, before invoking the next MessagePort callback.
  Preserve `onmessage`-before-registered-listeners order, listener snapshots,
  `once` removal, immediate-propagation handling, and event target cleanup.
- For every exception, call the worker-global legacy `onerror` handler before
  registered global `error` listeners. Only exact `true` from global
  `onerror` cancels owner forwarding. Global error listeners still run and
  observe the final `defaultPrevented` state.
- For each uncanceled exception, enqueue one owner `Worker` `ErrorEvent` with
  the existing worker URL/message attributes. Do not expose the original
  thrown JavaScript value in the page realm. Preserve command order relative
  to messages posted by later callbacks in the same worker turn.
- A callback exception does not cancel the MessagePort message, suppress the
  next listener, close or unroute the port, terminate the worker, or prevent a
  later message and reply.
- A process-backed transferred-port regression must cover: a handled Error
  thrown by `onmessage` with no owner error; an uncanceled primitive thrown by
  a registered listener with exactly one owner error; worker-global reporting
  order; later listener replies; owner event attributes/order; a subsequent
  healthy message; and worker survival.
- The existing SharedWorker callback regression must remain unchanged. Page
  MessagePort callback behavior must remain unchanged.

## Boundaries and tradeoffs

- Do not change SharedWorker, Service Worker, page, or BroadcastChannel error
  reporting in this slice. The shared dispatcher must select a reporter only
  for a native MessagePort and only when the current worker realm installs an
  owner-appropriate hook.
- Do not change worker scheduling, transfer ownership, structured cloning,
  SharedWorker callback reporting, startup failures, promise rejections,
  console reporting, error source locations, or specialized worker event
  targets.
- The focused regression is not complete DedicatedWorker, MessagePort,
  `EventTarget`, HTML error-reporting, or Web Platform Test conformance. Remote
  CI, cross-platform certification, and issue #40 production/release gates
  remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-793.md`

## Implementation and verification

The design contract is committed before implementation. Record the exact
process-backed regression, scoped Cargo commands, and any owner-event ordering
evidence here after implementation. Remote CI and complete worker/MessagePort
conformance remain issue #40 gates.
