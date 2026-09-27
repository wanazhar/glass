---
id: native-engine-browser-792
scope: glass-browser/shared-worker-message-port-callback-errors
status: completed
depends-on: [native-engine-browser-791]
---

# Glass native-engine browser slice 792: SharedWorker MessagePort delivery and callback errors

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [DOM event-listener invocation algorithm](https://dom.spec.whatwg.org/#concept-event-listener-inner-invoke)
  reports exceptions from listener callbacks to the callback realm's global;
  dispatch proceeds to later listeners unless propagation is stopped.
- The [HTML exception-reporting algorithm](https://html.spec.whatwg.org/multipage/webappapis.html#report-an-exception)
  reports at the worker global. Its dedicated-worker forwarding branch does
  not make a shared-global error an error event on each page's `SharedWorker`
  object.
- The [HTML channel-messaging contract](https://html.spec.whatwg.org/multipage/web-messaging.html#message-ports)
  dispatches `message` events at `MessagePort` targets. Slices
  [786](native-engine-browser-786.md) and
  [791](native-engine-browser-791.md) cover SharedWorker connect delivery and
  global reporting, but do not exercise a worker-side MessagePort receiver
  after connect-callback exceptions.

## Objective

First establish whether a SharedWorker's connected ports still deliver a
page-to-worker message after `onconnect`/`connect` callback exceptions, using
an actual worker-side message handler. Then report SharedWorker MessagePort
callback exceptions at the shared worker global without losing later listeners,
port delivery, or the shared runtime.

## Contract

- The process-backed regression installs a worker-side `MessagePort` receiver
  before claiming page-to-worker delivery. It sends through the port connected
  after an `onconnect` callback exception and verifies worker receipt and a
  reply to the page. This explicitly avoids treating a missing reply as a
  delivery failure when no worker reply handler exists.
- If that baseline fails, isolate the page command route, worker dispatch, and
  host-turn scheduling boundary in the same regression before changing those
  paths. Do not redesign task scheduling or the generic message transport
  unless the instrumented reproduction demonstrates that one is the cause.
- For `message` dispatch on a MessagePort owned by a SharedWorker, report an
  exception from `onmessage` or an individual `message` listener through the
  existing worker-global exception reporter before invoking the next callback.
  Preserve handler-before-listener order, listener snapshots, `once` removal,
  event target/currentTarget cleanup, and immediate-propagation behavior.
- Invoke the shared worker's legacy `onerror` before its registered global
  `error` listeners. Only exact `true` from `onerror` cancels the worker-global
  ErrorEvent; error listeners still run and observe the final cancellation
  state. Reporting a SharedWorker-global exception must not add an error event
  to any page `SharedWorker` object.
- An exception in a MessagePort callback must not cancel the `message` event,
  suppress later listeners, close the port, remove its bridge, terminate the
  shared runtime, or prevent a subsequent message/reply on that port.
- Cover a thrown `onmessage` Error handled by exact-`true` `onerror` and a
  thrown registered-listener primitive left uncanceled, later listeners and
  replies, global error-listener state/order, the post-connect-error
  page-to-worker round trip, and an empty page-owner error list in one focused
  process-backed test. Keep the existing no-error SharedWorker port regression
  as a separate baseline.

## Boundaries and tradeoffs

- Do not change page-owned, dedicated-worker, or Service Worker MessagePort
  callback reporting in this slice. The shared channel bootstrap is reused by
  several realms, so reporting must be explicitly available only to its
  SharedWorker owner rather than changing all MessagePort exception behavior
  as a side effect.
- Do not change SharedWorker startup, worker-global `connect` dispatch,
  structured cloning, transfer ownership, task-source arbitration, or generic
  scheduler behavior unless the focused delivery reproduction proves a defect
  in one of those boundaries. If delivery needs a scheduler redesign, record
  that evidence and split the scheduler work into its own contract.
- Dedicated/Service Worker callback semantics, `messageerror`, promise
  rejection reporting, console reporting, source locations, and complete
  MessagePort/Web Platform Test conformance remain outside this slice.
- This is a local behavior checkpoint, not complete worker conformance, native
  production readiness, remote CI, cross-platform certification, or issue #40
  closure evidence.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-792.md`

## Implementation and verification

The contract checkpoint is `1345c434`; implementation and the focused
process-backed regression are committed in `dcc8d31d`.

The initial no-reply observation in slice 791 was corrected after inspection:
that probe had no worker-side message receiver and therefore did not establish
a delivery defect. This slice's process-backed test installs a real receiver,
then proves delivery and reply after a connect callback exception. It also
proves that a throwing MessagePort `onmessage` callback is reported before
later message listeners and that a throwing registered listener is reported
before the next listener. Exact-`true` global cancellation, uncanceled error
state, continued replies, a later healthy message, and no page-owner errors
are all asserted.

Passed locally:

- `cargo check -p glass-browser --test native_engine --locked --quiet`.
- `cargo test -p glass-browser --test native_engine --locked --quiet -- native_content_process_shared_worker_message_port_callback_errors_recover --exact --test-threads=1` (1 passed).
- The freshly rebuilt `native_engine` integration binary passed
  `native_content_process_reports_shared_worker_connect_callback_errors_at_global`,
  `native_content_process_shared_worker_reuses_named_runtime_and_ports`, and
  `native_local_dedicated_worker_message_callback_errors_report_and_forward_in_order` (1 each).
- `rustfmt --edition 2024 --check` on `javascript.rs` and `native_engine.rs`,
  plus `git diff --check`.

The process-backed target emits existing native-DOM dead-code warnings.
Remote CI, full worker/MessagePort Web Platform Test conformance,
cross-platform certification, and issue #40's production/release gates remain
open.
