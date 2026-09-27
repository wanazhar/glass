---
id: native-engine-browser-791
scope: glass-browser/shared-worker-connect-callback-errors
status: in-progress
depends-on: [native-engine-browser-790]
---

# Glass native-engine browser slice 791: SharedWorker connect callback errors

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [DOM event-listener invocation algorithm](https://dom.spec.whatwg.org/#concept-event-listener-inner-invoke)
  reports a callback exception for its callback realm and continues with later
  listeners unless propagation was stopped.
- The [HTML exception-reporting algorithm](https://html.spec.whatwg.org/multipage/webappapis.html#report-an-exception)
  fires a cancelable `ErrorEvent` at the worker global. Its legacy `onerror`
  handler is called with the message, filename, line, column, and error; an
  exact `true` return cancels default reporting. If not handled, the special
  `Worker`-object forwarding task applies to dedicated workers; a shared worker
  error is not forwarded to each page's `SharedWorker` object.
- Slices [786](native-engine-browser-786.md) and
  [787](native-engine-browser-787.md) establish SharedWorker `connect` event
  shape/cancellation and the existing worker-global runtime-error reporter.
  Slice [789](native-engine-browser-789.md) applies that reporter to
  dedicated-worker `message` callbacks while preserving dedicated owner
  forwarding.

## Objective

Report exceptions thrown by SharedWorker `onconnect` and `connect` listeners at
the shared worker global, continue connection-event delivery, and preserve the
shared runtime for later connections.

## Contract

- Apply only to exceptions thrown by `SharedWorkerGlobalScope.onconnect` or an
  individual `connect` listener while dispatching the native SharedWorker
  connection `MessageEvent`.
- Report the original thrown JavaScript value synchronously through the
  existing shared worker global `__glassReportWorkerScriptError` path before
  invoking the next callback. The resulting worker-global `ErrorEvent` uses
  the existing bounded message and worker-URL metadata.
- Preserve `onconnect`-before-listeners order, the listener snapshot/order,
  and current event target/currentTarget/phase cleanup. A callback exception
  neither cancels the `connect` event nor suppresses later listeners or
  connections. Existing exact-`false` `onconnect` cancellation remains
  independent and visible to later listeners.
- For every callback exception, invoke the shared worker global's legacy
  `onerror` handler before registered global `error` listeners. Only exact
  `true` from `onerror` marks the worker-global `ErrorEvent` handled; global
  error listeners still run and observe the final `defaultPrevented` state.
- Keep an uncanceled SharedWorker callback exception inside the shared worker's
  global reporting boundary. Do not enqueue a `WorkerScriptError` to any page
  `SharedWorker` object or attribute it to one connection: HTML's dedicated
  worker forwarding branch does not apply to a shared worker global.
- An exception thrown while reporting an exception does not recursively start
  another report. Preserve the existing reporter's recursion guard.
- A thrown callback does not terminate the shared worker or discard its
  connection ports. Later listeners in that connect dispatch and a subsequent
  connection can still run and exchange a port message.
- Cover both an exception from `onconnect` and one from a registered `connect`
  listener, exact-`true` cancellation for one and uncanceled reporting for the
  other, later-listener delivery, global error-listener state, and absence of
  page `SharedWorker` error events in a process-backed same-name/two-connection
  regression.

## Boundaries and tradeoffs

- MessagePort `message` callbacks, Service Worker callbacks, SharedWorker
  startup/module-evaluation failures, promise rejections, worker console
  reporting, error source locations, other specialized worker EventTargets,
  and task-source arbitration are unchanged.
- Dedicated-worker callback exceptions retain their existing owner-forwarding
  behavior. This slice does not alter the generic worker scheduler or invent
  per-connection ownership for a shared-global runtime error.
- The local regression is not complete EventTarget, HTML exception-reporting,
  SharedWorker lifetime, or Web Platform Test conformance evidence. Issue #40's
  remaining profile, security, platform, product, CI, performance, and release
  gates stay open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-791.md`

## Implementation and verification

Design contract committed before implementation. Local validation and exact
evidence will be recorded here when the behavioral slice is complete. Remote
CI is not part of this local checkpoint.
