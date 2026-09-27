---
id: native-engine-browser-799
scope: glass-browser/worker-global-messageerror-delivery
status: completed locally
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
- Service Workers specify `messageerror` as an `ExtendableMessageEvent`;
  source/origin and event lifetime therefore cannot be claimed from the
  generic worker `MessageEvent` path alone
  ([Service Workers §4.7–4.8](https://w3c.github.io/ServiceWorker/#extendablemessageevent)).
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

Turn direct worker-global structured-clone decode failures into `messageerror`
events, expose the handler property, and keep the worker able to process later
valid messages. This is a delivery/error-recovery increment, not full
Service Worker message-event conformance.

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
  decoded data/ports. Dispatch one non-bubbling, non-cancelable event named
  `messageerror` at the worker global with `data: null` and `ports: []`; set
  `target` and `currentTarget` to that global during callbacks and reset
  dispatch state afterward. Dedicated Worker delivery uses its current
  `MessageEvent` shell. The current Service Worker path also uses the existing
  generic worker `MessageEvent` shell and empty-origin/null-source metadata;
  this is explicitly incomplete Service Worker behavior, not a claim of
  `ExtendableMessageEvent` conformance.
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
- Add process-backed HTTP(S) regressions for Dedicated Worker and Service
  Worker global delivery. Inject a bounded malformed clone-node envelope with
  no transfer ports or object-URL transfers through the internal dispatcher;
  verify `onmessageerror` reflection/conversion/order, event identity and
  bounded current event fields, that `message` handlers do not receive the
  failed decode, that later listeners still run, and that a later ordinary
  valid message is delivered. The Service Worker regression proves only this
  dispatcher/recovery increment; it does not certify the required
  `ExtendableMessageEvent` interface, client source/origin, or `waitUntil()`
  lifetime. This internal malformed-envelope fixture is not evidence that web
  content can bypass sender-side `postMessage()` serialization checks.
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
- A required follow-up must model Service Worker `message` and `messageerror`
  as `ExtendableMessageEvent`s, propagate the initiating client as `source`
  and its origin, and settle `waitUntil()` lifetime promises. Until then, the
  new Service Worker failure event uses the same known-incomplete generic
  event shell as the existing direct Service Worker `message` path.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-799.md`

## Implementation and verification

Contract checkpoint: `b3c3727f`; implementation and process-backed
regressions: `af675938`.

The direct worker-global dispatcher now catches failures from
`glassMessageDecodeEnvelope()` and emits a `messageerror` event with null data
and no ports instead of leaking a partial `message` or aborting the worker
turn. `onmessageerror` is backed by an ordered slot that survives worker
bootstrap re-entry; replacement preserves order, nullish/primitive clearing
deactivates the slot, reactivation appends a new one, non-callable objects are
retained as no-op handlers, and an active-dispatch snapshot skips a slot that
was cleared during delivery. Messageerror callback exceptions follow the
existing worker-global reporting/forwarding policy and do not stop later
listeners.

The process-backed HTTP(S) regression
`native_content_process_worker_dispatches_messageerror_and_recovers` verifies
Dedicated Worker event type/fields, handler `this` and identity, listener
delivery, no owner error for a decode failure, and a later valid message. The
extended `native_content_process_service_worker_message_callback_errors_stay_global`
regression injects malformed envelopes from the Service Worker global and
checks property conversion, replacement/clear/reactivation order, snapshot
deactivation, global exception reporting, listener continuation, later worker
messages, active state, and no client-facing error. Its generic Service
Worker `MessageEvent` shell and empty-origin/null-source metadata are an
explicitly incomplete compatibility path, not evidence for the required
`ExtendableMessageEvent`, source/origin, or `waitUntil()` behavior.

Passed locally:

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine --locked native_content_process_service_worker_message_callback_errors_stay_global -- --exact` (1 passed)
- `target/debug/deps/native_engine-d4489efcb3153c24 native_content_process_worker_dispatches_messageerror_and_recovers --exact` (1 passed)
- Adjacent existing regressions: Service Worker MessagePort callback recovery (1 passed) and large Dedicated Worker message delivery (1 passed).
- `cargo fmt --all -- --check`, `git diff --check`
- Release-truth: 1,427 Markdown files, zero current-claim failures; docs depth: 93 current guides/19 contracts; shortcut inventory: 15 keys/63 markers; docs coverage: 346 MCP tools, 17 examples, 22 public modules.

Remote CI was not run; these commits remain local. Full Service Worker
`ExtendableMessageEvent`/source/origin/lifetime semantics, MessagePort and
page-side Worker-proxy decode failures, object-URL/port rollback for malformed
transfers, complete Web IDL/EventTarget behavior, cross-platform certification,
and WPT remain open issue #40 work.
