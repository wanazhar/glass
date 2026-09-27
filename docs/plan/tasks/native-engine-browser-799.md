---
id: native-engine-browser-799
scope: glass-browser/worker-global-messageerror-delivery
status: contracted
depends-on: [native-engine-browser-798]
---

# Glass native-engine browser slice 799: Worker-global messageerror delivery

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- HTML's [`MessageEventTarget` mixin](https://html.spec.whatwg.org/multipage/web-messaging.html#messageeventtarget-mixin)
  defines both `onmessage` and `onmessageerror` event-handler attributes.
  When structured deserialization fails, HTML dispatches `messageerror` at the
  receiving target instead of dispatching `message`.
- The shared native worker bootstrap dispatches direct messages to
  `DedicatedWorkerGlobalScope` and `ServiceWorkerGlobalScope` through
  `__glassDispatchWorkerMessage(payload)`. It currently lets
  `glassMessageDecodeEnvelope()` exceptions escape and exposes no
  `onmessageerror` property.
- Slice [798](native-engine-browser-798.md) establishes ordered handler slots
  for Service Worker lifecycle properties; slices [795](native-engine-browser-795.md)
  and [797](native-engine-browser-797.md) establish worker-global callback
  exception reporting and continuation behavior.

## Objective

Turn a worker-global structured-clone decode failure into a `messageerror`
event, expose the corresponding worker-global handler property, and keep the
worker able to process later valid messages.

## Contract

- Apply to failures raised while `worker_bootstrap()` decodes a direct
  `__glassDispatchWorkerMessage(payload)` delivery for a Dedicated Worker or
  Service Worker global. The SharedWorker global may expose the standard
  property through the shared bootstrap, but SharedWorker `connect` ports and
  their messages are not this direct-global delivery path.
- Catch only the exception from decoding the incoming envelope. Do not catch
  or reinterpret sender-side structured serialization/transfer-list failures:
  `postMessage()` must keep its existing synchronous failure behavior. Do not
  change the page-side `Worker` proxy dispatcher, Service Worker client-side
  event dispatch, MessagePort delivery, or SharedWorker port delivery.
- On decode failure, do not dispatch a `message` event or expose partially
  decoded data/ports. Instead dispatch one `MessageEvent` named
  `messageerror` at the worker global with `data: null`, `ports: []`, empty
  origin, null source, and the existing non-bubbling, non-cancelable message
  event shape. Set `target` and `currentTarget` to the worker global during
  callbacks and reset dispatch state afterward.
- Expose `onmessageerror` on worker globals, initially `null`. Apply the
  `EventHandler` conversion: nullish and primitive assignments clear it; a
  callable value is invoked with the worker global as `this` and the event as
  its argument; a non-callable object remains the reflected value but invokes
  no callback. Model an active property as one stable listener slot in the
  `messageerror` listener order: replacement keeps its position, clearing
  deactivates it, and later reactivation appends at the new position. A
  dispatch snapshot must skip a slot cleared during that dispatch.
- Continue registered `messageerror` listeners after callback exceptions and
  use the same worker-global reporting/forwarding policy already used for
  direct `message` callbacks. Preserve Dedicated Worker forwarding and the
  Service Worker global-only boundary; do not terminate the worker because
  decoding failed.
- Add process-backed HTTP(S) regressions for both Dedicated Worker and Service
  Worker global delivery. Inject a bounded malformed clone-node envelope with
  no transfer ports or object-URL transfers through the internal dispatcher;
  verify `onmessageerror` reflection/conversion/order, event identity and
  fields, that `message` handlers do not receive the failed decode, that later
  listeners still run, and that a later ordinary valid message is delivered.
  This internal malformed-envelope fixture is not evidence that web content
  can bypass sender-side `postMessage()` serialization checks.
- MessagePort/SharedWorker-port `messageerror`, parent-side Worker-object
  decoding, Service Worker client-side `messageerror`, object-URL/port cleanup
  on malformed transferred envelopes, generic EventTarget/Web IDL
  conformance, and full WPT behavior remain separate work.

## Boundaries and tradeoffs

- Keep the failure event on the receiving worker global; do not forward a
  Service Worker decode failure to clients, registrations, or containers.
- Keep the malformed fixture transfer-free so this slice does not imply a
  resource rollback contract for partially revived ports or object URLs.
- Do not change queue/task-source timing, structured-clone serialization,
  limits, worker startup/shutdown, script error reporting outside message
  callbacks, or Service Worker lifecycle settlement.
- This contract does not establish complete Worker messaging, Web IDL,
  EventTarget, cross-platform, remote-CI, or WPT conformance. Issue #40 remains
  open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-799.md`

## Implementation and verification

Contract checkpoint only. Implementation and test evidence are pending.
