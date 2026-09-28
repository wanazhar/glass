---
id: native-engine-browser-807
scope: glass-browser/service-worker-message-port-bridge-close
status: in-progress
depends-on: [native-engine-browser-806]
---

# Glass native-engine browser slice 807: Service Worker MessagePort bridge close

## Objective

Implement explicit `MessagePort.close()` across a page-to-Service-Worker bridge
owned by `NativeServiceWorkerRegistry`, delivering one generic `close` Event
to the still-open endpoint in the other realm and retiring only that route.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML Standard's MessagePort disentangle steps](https://html.spec.whatwg.org/multipage/web-messaging.html#message-ports)
  sever the entanglement and fire `close` at the other port. The initiating
  endpoint does not receive its own close event.
- Slice [806](native-engine-browser-806.md) implements the same behavior for
  `NativeWorkerRegistry`. Service Worker bridge keys belong to a separate
  `NativeServiceWorkerRegistry`; applying close through the Worker registry
  would cross ownership. The current Service Worker path rejects matching
  close commands explicitly.
- Service Worker-originated MessagePort messages and close records use the
  bounded `NativeMessagePortPageMessage` queue. Page-originated close commands
  are consumed by `apply_page_message_port_commands` and must dispatch into
  the owning Service Worker runtime.

## Contract

- Scope is explicit close of a live page-to-Service-Worker MessagePort bridge
  registered in `NativeServiceWorkerRegistry`. Keep Dedicated/Shared Worker
  bridge routing in `NativeWorkerRegistry`; do not add a fallback between the
  registries or change same-realm Slice 805 behavior.
- Validate the bridge key, command realm, and Service Worker route owner before
  mutating state. A page command may close a route in this page's registry. A
  Service Worker command may close only a route owned by its exact worker ID.
  Wrong-owner commands fail without retiring a route; an unknown or already
  retired key is idempotent and cannot affect another route.
- For page-initiated close, retire the matching Rust route and purge pending
  work for that key, then dispatch one generic `Event` named `close` at the
  surviving MessagePort in the owning Service Worker realm through its normal
  event callback path. For Service Worker-initiated close, retire the same
  route, purge queued messages for it, and enqueue one close record for the
  surviving page endpoint through the existing page MessagePort dispatcher.
- The JavaScript initiator is closed locally and does not receive its own
  close event. The opposite endpoint remains open but disentangled, loses the
  host bridge mapping, and receives exactly one generic close Event through
  `onclose` and registered listeners. Repeated close must not duplicate the
  event. No message may cross the retired route; other Service Worker bridge
  keys remain live.
- Preserve bounded capacity in the Service Worker registry. Enforce
  `live_routes + queued_events <= MAX_NATIVE_WORKER_MESSAGES`, reserving one
  queue slot per live route for its close notification. Normal queued messages
  and newly transferred routes must account for that reservation before
  state mutation. Retiring a route purges that route's queued messages and
  converts its reservation into the peer close record, so a committed close
  cannot fail merely because the event queue is full.
- Add process-backed HTTP regression coverage for both close directions using
  a real registered/active Service Worker. Check event identity and dispatch
  state through both handler and listener paths, peer openness/disentanglement,
  repeated close, route-specific message suppression, and an unrelated
  Service Worker bridge that continues to exchange messages. Keep this as
  Service Worker evidence; do not infer SharedWorker or full WPT conformance.
- If the recorded route's Service Worker no longer exists, retire only that
  stale route without dispatching into a different worker. Do not silently
  treat an owner mismatch as a stale route. Event callback failures follow the
  existing Service Worker callback reporting and recovery contract.
- Queue/task-source ordering beyond this route's accepted messages and close
  notification remains an open GCWP/WPT requirement. This slice does not
  certify that all HTML task-source ordering is implemented.

## Boundaries and tradeoffs

- Keeping route ownership inside `NativeServiceWorkerRegistry` prevents a
  page-controlled key from deleting a Dedicated/Shared Worker route or a
  Service Worker from closing another owner's endpoint.
- Reserving queue capacity for every live route reduces the number of normal
  queued messages available under high bridge counts. It guarantees close
  notification capacity and keeps the existing queue as the sole event path.
- Service Worker process-backed coverage proves the active single-page route
  used by this fixture. It does not certify multi-client Service Worker port
  scheduling, worker termination/document-destruction disentangling, garbage
  collection, full task-source ordering, generic EventTarget/Web IDL, WPT,
  remote CI, or cross-platform behavior.

## Paths

- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`

## Verification

Implementation and focused verification are pending. Record exact commands and
results here. Remote CI, WPT, and cross-platform certification are separate
issue #40 gates; local verification must not be reported as remote evidence.
