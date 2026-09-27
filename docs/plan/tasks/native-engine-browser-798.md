---
id: native-engine-browser-798
scope: glass-browser/service-worker-lifecycle-handler-properties
status: in_progress
depends-on: [native-engine-browser-797]
---

# Glass native-engine browser slice 798: Service Worker lifecycle handler properties

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- Service Worker specification §4.1.5 requires `oninstall` and `onactivate`
  event-handler IDL attributes on `ServiceWorkerGlobalScope`:
  [Service Workers](https://w3c.github.io/ServiceWorker/).
- HTML's event-handler processing model treats IDL handlers as ordinary
  listeners in the target's registration order. Replacing a non-null handler
  preserves its listener slot; clearing deactivates/removes it; reactivation
  registers it at the later position. See
  [HTML event handlers, §8.1.8](https://html.spec.whatwg.org/multipage/webappapis.html).
- `EventHandler` uses `LegacyTreatNonObjectAsNull`: nullish and non-object
  values clear the handler; a non-callable object is retained but acts as a
  no-op callback. See
  [Web IDL callback function types](https://webidl.spec.whatwg.org/#LegacyTreatNonObjectAsNull).
- Slice [797](native-engine-browser-797.md) establishes global-only reporting
  for synchronous exceptions from registered lifecycle listeners and
  preserves the lifecycle dispatch's `waitUntil()` settlement.
- The current native bootstrap has a special `onfetch` property, but does not
  expose `oninstall` or `onactivate`; lifecycle dispatch invokes only the
  registered-listener collection.

## Objective

Expose `ServiceWorkerGlobalScope.oninstall` and `.onactivate` and dispatch
their handlers in the same ordered listener sequence as registered lifecycle
listeners, while preserving Slice 797 error reporting and `waitUntil()`.

## Contract

- Add only the `oninstall` and `onactivate` Service Worker global properties.
  Each initially reads as `null`. Callable assignment stores and exposes that
  function. Nullish or primitive assignment clears the event handler according
  to `EventHandler` conversion. A non-callable object remains the exposed
  handler value and registers an event-handler listener that performs no
  callback work. Do not add the properties to page, DedicatedWorker, or
  SharedWorker globals.
- Model each active property handler as one stable event listener in the same
  ordered listener collection used by `addEventListener()` for its event type.
  The first non-null activation inserts it at that point in registration
  order. Replacing its value does not move it. Clearing it removes/deactivates
  the slot; assigning a non-null value afterward creates a new slot at the
  later position. Callback snapshots must not invoke a deactivated handler.
- Invoke a callable property value with `this === ServiceWorkerGlobalScope`
  and the dispatched lifecycle event as its one argument. The handler and
  registered callbacks receive the same event object and may call its existing
  `waitUntil()` method. Preserve listener order and `waitUntil()` fulfillment
  or rejection handling.
- A synchronous exception from a callable `oninstall` or `onactivate` handler
  goes through the existing worker-global
  `__glassReportWorkerScriptError` path before dispatch continues. Do not
  forward it to clients or enqueue a dedicated-worker error command.
- Add process-backed HTTP(S) coverage proving initial null values, install
  and activate property invocation/`this`/event type, assignment replacement
  without movement, null removal and later reactivation order, primitive
  clearing and non-callable-object no-op conversion, handler exception
  reporting, listener continuation, fulfilled `waitUntil(skipWaiting())` and
  `waitUntil(clients.claim())`, activation, page control, and no page error
  event.
- DedicatedWorker/SharedWorker event dispatch, Service Worker `onfetch`,
  `onmessage`, `onmessageerror`, `ExtendableEvent`/event object identity,
  rejected-lifetime behavior, lifecycle persistence across process restart,
  and generic Web IDL/EventTarget semantics remain unchanged.

## Boundaries and tradeoffs

- Persist the property slot/value only for the lifetime of the current native
  Service Worker global; script evaluation and subsequent lifecycle turns in
  that same global must retain it. Do not add profile persistence or change
  worker termination/restart behavior.
- Do not add inline event-handler content attributes, expand the general
  worker event-handler inventory, change lifecycle event shapes/default
  actions, rewrite the existing `onfetch` dispatcher, change task-source or
  `waitUntil()` semantics, or modify startup/promise-rejection/console error
  reporting.
- This slice does not claim complete Service Worker lifecycle, Web IDL,
  EventTarget, HTML error-reporting, cross-platform, remote-CI, or WPT
  conformance. Those remain issue #40 gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-798.md`

## Implementation and verification

Commit this contract before implementation. Record the process-backed
regression and scoped local verification commands here after implementation.
Remote CI and complete Service Worker lifecycle/WPT conformance remain issue
#40 gates.
