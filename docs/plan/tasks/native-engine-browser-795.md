---
id: native-engine-browser-795
scope: glass-browser/service-worker-message-callback-errors
status: completed
depends-on: [native-engine-browser-794]
---

# Glass native-engine browser slice 795: Service Worker message callback errors

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [DOM event-listener invocation algorithm](https://dom.spec.whatwg.org/#concept-event-listener-inner-invoke)
  reports a callback exception at that callback's global and continues through
  the listener list unless immediate propagation was stopped.
- The [HTML exception-reporting algorithm](https://html.spec.whatwg.org/multipage/webappapis.html#report-an-exception)
  fires a cancelable `ErrorEvent` at the relevant global. Owner-`Worker`
  forwarding is specific to `DedicatedWorkerGlobalScope`; a Service Worker
  callback error remains at its `ServiceWorkerGlobalScope`.
- The generic worker bootstrap currently reports global `message` callback
  exceptions only when its dedicated-worker capture mode is enabled. The
  Service Worker bootstrap disables that mode to avoid dedicated-owner
  forwarding. Slice [794](native-engine-browser-794.md) separately enables
  global-only reporting for Service Worker MessagePort callbacks.

## Objective

Report exceptions thrown by a Service Worker global `onmessage` handler or
registered `message` listener at that ServiceWorkerGlobalScope. Continue later
listeners and later message events without forwarding these errors to clients.

## Contract

- Apply only to `message` callbacks dispatched on the Service Worker global:
  its `onmessage` handler and registered `message` listeners. Do not change
  MessagePort callback behavior from slice 794.
- Report each thrown JavaScript value through the existing worker-global
  `__glassReportWorkerScriptError` path before invoking the next callback.
  Preserve handler-before-listener order and existing listener iteration
  behavior.
- Invoke the Service Worker global's legacy `onerror` before registered
  global `error` listeners. Only exact `true` from `onerror` cancels the
  cancelable global ErrorEvent; later error listeners still run and observe
  the final `defaultPrevented` state.
- Never enqueue the dedicated-worker `WorkerScriptError` owner command for a
  Service Worker global callback exception. Do not dispatch that failure to a
  page `Worker`, `ServiceWorker`, client, registration, or
  `ServiceWorkerContainer` object.
- A thrown callback must not stop later `message` listeners for that same
  event, terminate or deactivate the Service Worker, or prevent a later
  independent client message from being handled.
- Add a process-backed HTTP(S) regression in which a controlled page sends a
  message to its active Service Worker. Cover a handled Error from global
  `onmessage`, an uncanceled primitive from a registered listener, global
  error-report order/state, continued later-listener response delivery,
  successful handling of a subsequent message, active worker state, and no
  client/container/ServiceWorker-object error event.
- DedicatedWorker, SharedWorker, page, MessagePort, BroadcastChannel, and
  generic scheduler behavior must remain unchanged.

## Boundaries and tradeoffs

- Do not change Service Worker `fetch` callback dispatch, `waitUntil()`
  lifetime, registration/update behavior, MessagePort callbacks, promise
  rejection reporting, console reporting, error source locations, event
  `source` modeling, or client selection semantics.
- Do not alter structured cloning, client routing, worker task sources, worker
  lifecycle, page command processing, or general scheduler behavior unless the
  focused regression proves one of these boundaries is defective.
- The focused regression is not complete Service Worker, Worker, HTML error
  reporting, `EventTarget`, or Web Platform Test conformance. Remote CI,
  cross-platform certification, and issue #40 production/release gates remain
  open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-795.md`

## Implementation and verification

Contract checkpoint: `3fa1ac97`; implementation and process-backed regression:
`6ec47c4c`.

The worker bootstrap separates global exception reporting from dedicated-owner
forwarding. DedicatedWorker retains both behaviors, SharedWorker retains its
global-only connect behavior, and Service Worker global `message` and
MessagePort callback errors use global-only reporting. The regression
`native_content_process_service_worker_message_callback_errors_stay_global`
uses a controlled HTTP(S) page and verifies an exact-`true` handled Error from
`onmessage`, an uncanceled primitive from a registered listener, global error
order/state, later listener client replies, a later healthy message, active
worker state, and no client/container/ServiceWorker-object error event.

The first test run found the fixture had not claimed the registering page; the
fixture now performs `skipWaiting()` and `clients.claim()`. A second run showed
that awaiting a page promise for a posted Service Worker message leaves no
native host operation pending within that script evaluation. The test now
sends the message synchronously and pumps bounded page turns while awaiting a
client reply. Both were test-harness issues; the corrected regression passes.

Passed locally:

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine --locked --quiet -- native_content_process_service_worker_message_callback_errors_stay_global --exact --test-threads=1` (1 passed).
- Direct process-backed regressions from the rebuilt native-engine test binary: `native_content_process_shared_worker_message_port_callback_errors_recover`, `native_content_process_service_worker_message_port_callback_errors_stay_global`, `native_content_process_dedicated_worker_message_port_callback_errors_report_and_forward`, and `native_content_process_reports_shared_worker_connect_callback_errors_at_global` (1 passed each).
- Direct DedicatedWorker global-message regression `native_local_dedicated_worker_message_callback_errors_report_and_forward_in_order` (1 passed).
- Rustfmt check for both touched Rust files and `git diff --check`.

The integration target emits existing native-DOM dead-code warnings. Remote CI
and complete Service Worker, Worker, HTML error-reporting, EventTarget, and WPT
conformance remain issue #40 gates.
