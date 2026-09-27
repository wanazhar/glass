---
id: native-engine-browser-794
scope: glass-browser/service-worker-message-port-callback-errors
status: completed
depends-on: [native-engine-browser-793]
---

# Glass native-engine browser slice 794: Service Worker MessagePort callback errors

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [DOM event-listener invocation algorithm](https://dom.spec.whatwg.org/#concept-event-listener-inner-invoke)
  reports an exception for the callback realm's global and continues through
  the listener list unless immediate propagation was stopped.
- The [HTML exception-reporting algorithm](https://html.spec.whatwg.org/multipage/webappapis.html#report-an-exception)
  fires a cancelable `ErrorEvent` at that global. Its owner-`Worker`
  forwarding branch applies to `DedicatedWorkerGlobalScope`; it does not turn
  a Service Worker global error into an error event on each client.
- Slice [792](native-engine-browser-792.md) reports SharedWorker MessagePort
  callback exceptions at the shared global. Slice
  [793](native-engine-browser-793.md) adds the DedicatedWorker global and
  owner-forwarding mode. The Service Worker bootstrap currently installs no
  MessagePort callback reporter.

## Objective

Report exceptions from a MessagePort `onmessage` handler or registered
`message` listener in a Service Worker at its `ServiceWorkerGlobalScope`. Keep
the error at that global, continue later port callbacks, and preserve the
worker and port for later messages.

## Contract

- Apply only to native MessagePort callbacks running in a Service Worker
  global. Do not infer the owner from the page endpoint or reuse dedicated-
  worker forwarding behavior.
- Report each thrown JavaScript value through the existing worker-global
  `__glassReportWorkerScriptError` path before invoking the next MessagePort
  callback. Preserve handler-before-listener order, listener snapshots,
  `once` removal, immediate-propagation behavior, and event target cleanup.
- Call the Service Worker global's legacy `onerror` handler before its
  registered global `error` listeners. Only exact `true` from `onerror`
  cancels that global ErrorEvent. Later error listeners still run and observe
  the final `defaultPrevented` state.
- Do not enqueue a dedicated `WorkerScriptError` owner command. Do not dispatch
  a callback error to a page `Worker`, `ServiceWorker`, client, registration,
  or `ServiceWorkerContainer` object. The exception remains within the Service
  Worker global reporting boundary.
- A callback exception does not cancel the port message, skip later port
  listeners, close or unroute the port, terminate the Service Worker, or
  prevent a later request/reply on the same port.
- A process-backed transferred-port test must cover an exact-`true` handled
  Error from `onmessage`, an uncanceled primitive from a registered listener,
  worker-global report/callback order, continued replies, a later healthy
  message, worker survival, and absence of client-side error forwarding.
- DedicatedWorker, SharedWorker, page MessagePort, BroadcastChannel, and
  generic task-scheduling behavior must remain unchanged.

## Boundaries and tradeoffs

- Do not change Service Worker global `message`/`fetch` event callback
  dispatch, `waitUntil()` lifetime, registration/update behavior, promise
  rejection reporting, console reporting, or error source locations.
- Do not change structured cloning, transfer ownership, page command routing,
  host task-source selection, or generic scheduler behavior unless the focused
  regression proves one of these boundaries is defective.
- The focused regression is not complete Service Worker, MessagePort,
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
- `docs/plan/tasks/native-engine-browser-794.md`

## Implementation and verification

Contract checkpoint: `a93b89a6`; implementation and process-backed regression:
`54d5693e`.

The Service Worker MessagePort callback reporter uses the existing global
worker-script error path with owner forwarding disabled. The regression
`native_content_process_service_worker_message_port_callback_errors_stay_global`
covers a handled `onmessage` Error, an uncanceled listener primitive, global
report order, later listener execution, replies after both exceptions, a later
healthy message, active Service Worker state, and no client-side errors.

Passed locally:

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine --locked --quiet -- native_content_process_service_worker_message_port_callback_errors_stay_global --exact --test-threads=1`
- Direct process-backed regression `native_content_process_service_worker_transfers_message_port_round_trip` (1 passed).
- Rustfmt check for the touched JavaScript and native-engine test sources; `git diff --check`.

The integration target emits existing native-DOM dead-code warnings. Remote CI
and complete Service Worker, MessagePort, and WPT conformance remain issue #40
gates.
