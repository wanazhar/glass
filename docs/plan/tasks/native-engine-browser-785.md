---
id: native-engine-browser-785
scope: glass-browser/dedicated-worker-message-handler-cancellation
status: completed
depends-on: [native-engine-browser-784]
---

# Glass native-engine browser slice 785: dedicated-worker message handler cancellation

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML event-handler processing algorithm](https://html.spec.whatwg.org/multipage/webappapis.html#event-handlers-on-elements,-document-objects,-and-window-objects)
  sets an event's canceled flag when an ordinary `EventHandler` callback
  returns exactly `false`; it then continues dispatch to later listeners.
- The [HTML WorkerGlobalScope interface](https://html.spec.whatwg.org/multipage/workers.html#workerglobalscope-and-workerglobalscope-mixin)
  extends `EventTarget`, and dedicated worker message handling is exposed on
  that worker global. The host-delivered message path is currently a separate
  native dispatcher from the page DOM event implementation.

## Objective

Make dedicated worker host-message dispatch produce the worker-realm message
event object and honor `onmessage` exact-false cancellation before invoking
later listeners, without changing worker task scheduling or applying
EventHandler return processing to ordinary listener callbacks.

## Contract

- A host-delivered dedicated-worker message is a worker-realm `MessageEvent`
  with its cloned `data` and transferred `ports`, worker-global `target` and
  `currentTarget` during dispatch, and the existing message-event defaults.
- The worker-global `onmessage` handler runs before registered message
  listeners. An exact `false` return sets `defaultPrevented` even though the
  message event is non-cancelable. `0`, `null`, `undefined`, and other values
  do not cancel.
- Later listeners still run and observe cancellation from the property
  handler. A listener callback's own `false` return remains ignored.
- Host message ordering, task budgets, structured-clone ownership, and
  transfer semantics do not change.

## Boundaries and tradeoffs

- This covers the dedicated worker's host-delivered `message` event path. It
  does not complete other `WorkerGlobalScope` handler attributes, shared-worker
  `connect` event construction or cancellation, worker CSP/error handling,
  callback exception reporting, EventTarget propagation/options, worker event
  scheduling, or specialized XHR/WebSocket/EventSource dispatchers.
- Full worker and Web Platform Test conformance and issue #40 production gates
  remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-785.md`

## Verification

Passed locally:

- `rustfmt --edition 2024 --check crates/glass-browser/src/browser/native_engine/javascript.rs crates/glass-browser/tests/native_engine.rs`
- `cargo check -p glass-browser --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine --locked --quiet -- native_local_dedicated_worker_onmessage_false_cancels_before_listeners --exact --test-threads=1`
  (1 passed; worker `MessageEvent`, non-cancelable exact-false cancellation,
  later-listener ordering, ignored listener return, and non-false result)
- `python3 scripts/check-documentation-coverage.py`
  (1,413 Markdown files; coverage validated)
- `git diff --check`

Remote CI, full worker/WPT conformance, cross-platform certification, and issue
#40 native-only production gates remain open.
