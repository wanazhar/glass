---
id: native-engine-browser-800
scope: glass-browser/service-worker-extendable-message-events
status: completed locally
depends-on: [native-engine-browser-799]
---

# Glass native-engine browser slice 800: Service Worker extendable message events

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The Service Workers specification assigns `ExtendableMessageEvent` to both
  Service Worker `message` and `messageerror`. It inherits from
  `ExtendableEvent` (not `MessageEvent`) and carries `data`, `origin`,
  `lastEventId`, `source`, and `ports`
  ([§4.7–4.8](https://w3c.github.io/ServiceWorker/#extendablemessageevent)).
- `ExtendableEvent.waitUntil()` accepts lifetime promises only while the event
  is trusted and active; activity continues after dispatch while at least one
  lifetime promise remains pending. When none remain, later calls throw
  `InvalidStateError`
  ([§4.4.1](https://w3c.github.io/ServiceWorker/#dom-extendableevent-waituntil)).
- The current page-to-worker path is
  `ServiceWorker.postMessage()` → `NativeServiceWorkerRegistry::post_message`
  → `NativeJavaScriptRuntime::dispatch_service_worker_message` →
  `__glassDispatchWorkerMessage`. The registry owns the current client ID and
  client projection, but the current event drops source/origin metadata and
  uses the generic worker `MessageEvent` shell.
- Lifecycle and fetch events have separate promise-settlement paths. The
  Service Worker message dispatcher currently returns no event-lifetime
  promise. Slice [799](native-engine-browser-799.md) added decode-failure
  recovery but explicitly left this event contract open.

## Objective

Give page-originated Service Worker `message` and decode-failure
`messageerror` events the bounded `ExtendableMessageEvent` contract, preserve
the sending client's identity and origin, and keep the worker event alive
through its `waitUntil()` promises.

## Contract

- Scope is the current page/client-to-Service-Worker path initiated by
  `ServiceWorker.postMessage()`. Both successfully decoded `message` and
  structured-clone decode-failure `messageerror` use an
  `ExtendableMessageEvent` instance. It is an `Event`/`ExtendableEvent`, not a
  `MessageEvent` subclass. Events are trusted when emitted by the native host,
  target the Service Worker global, and do not bubble or cancel.
- Populate `data`, `origin`, `lastEventId`, `source`, and `ports` from the
  decoded message and trusted sender metadata. `origin` is the initiating
  client's serialized origin, never the Service Worker script URL. `source`
  resolves from the registry-owned current client ID and that worker's
  validated client projection; it exposes the current projected client's
  stable ID/URL and working `postMessage()` route. Event access to `source`
  and `ports` is stable; `ports` is a frozen array. A failed decode exposes
  `data: null` and no transferred ports while retaining sender metadata; it
  must not dispatch `message` or expose partial data.
- Expose `ExtendableEvent` and `ExtendableMessageEvent` in the Service Worker
  global with the declared inheritance and constructor defaults. Constructor-
  created events are untrusted and cannot extend a host event's lifetime.
  `waitUntil()` applies Web IDL Promise conversion and throws
  `InvalidStateError` for an untrusted or inactive event. An event is active
  during dispatch and while any lifetime promise remains pending, allowing a
  pending promise's continuation to register another lifetime promise. Once
  dispatch has ended and the pending count reaches zero, later calls throw.
- Native dispatch waits for all registered lifetime promises, including
  promises added while another one keeps the event active, before completing
  the worker event turn. Promise fulfillment completes normally. Rejection is
  observed and drained through the event lifetime; it must not escape as a
  synchronous exception from the sender's `ServiceWorker.postMessage()`,
  become a page/client `error` event, stop later message listeners, or kill
  the Service Worker. Message events define no application response channel
  for lifetime rejection, so this slice records no extra user-visible failure
  signal. This is the bounded native policy for this event type, not a claim
  that the specification defines a message-specific rejection result.
- Reuse the existing Service Worker host settlement loop for cache, fetch, and
  client-message commands produced by the message event's lifetime work. Keep
  sender-side structured serialization/transfer errors synchronous and
  unchanged; preserve normal transferred-port delivery.
- Add process-backed HTTP(S) coverage for normal messages. Check event
  constructor/prototype identity and fields, sender origin/source ID and URL,
  a functional source reply, frozen ports, and `waitUntil()` with an
  asynchronous host operation, continuation-added work, late-call
  `InvalidStateError`, a rejected lifetime promise, and later-message delivery
  without a client error or worker termination. Exercise malformed internal
  clone envelopes at the runtime dispatch boundary; the page serializer cannot
  produce such an envelope, so no public corruption hook is added.
- Do not broaden this slice to ServiceWorkerClient-to-page `postMessage`,
  MessagePort/SharedWorker-port events, worker-client or MessagePort source
  variants, page-side Worker proxy decoding, transfer rollback, all
  `Client`/`WindowClient` Web IDL, global EventTarget conformance, or full WPT,
  platform, and remote-CI certification.

## Boundaries and tradeoffs

- `ExtendableMessageEvent` deliberately does not inherit from `MessageEvent`;
  the two interfaces expose similar message fields but have distinct standard
  inheritance and lifetime contracts.
- Lifetime rejection is not promoted into a page-visible `error` event or a
  synchronous `postMessage()` failure because this legacy Service Worker
  message event has no operation-specific response/failure channel. Treating
  it as callback failure would conflate promise settlement with the existing
  worker-global exception reporter.
- Sender origin must cross the native API boundary as trusted context
  metadata. Deriving it from the worker URL or accepting it from message data
  would misattribute the sender and is not permitted.
- This bounded integration does not certify the complete Client interfaces,
  arbitrary Service Worker source kinds, delayed timers, or the browser's
  complete Service Worker lifecycle policy.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`

## Implementation and verification

The page-to-worker command now carries the page's serialized origin. The
registry verifies it against both the current client URL and registration
origin, resolves the sender from its validated client projection, and only
then registers transfer routes. The runtime dispatches through a dedicated
Service Worker message path, constructs `ExtendableMessageEvent` for both
`message` and decode-failure `messageerror`, and waits for its active lifetime
promises. The host dispatcher is captured and removed from the worker global
before worker-authored script runs, so worker code cannot forge a trusted host
dispatch. Rejected lifetime promises are consumed without changing the
sender's synchronous result or reporting a client/global error. The existing
host settlement loop continues to process fetch, cache, and client-message
commands from the event lifetime; transferred MessagePort delivery remains
unchanged.

Passed locally:

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine native_content_process_service_worker_message_events_extend_lifetime --locked --quiet` (1 passed)
- `cargo test -p glass-browser --lib malformed_service_worker_message_dispatches_extendable_messageerror --locked --quiet` (1 passed)
- `cargo test -p glass-browser --test native_engine native_content_process_service_worker_transfers_message_port_round_trip --locked --quiet` (1 passed)
- `cargo test -p glass-browser --test native_engine native_content_process_service_worker_message_callback_errors_stay_global --locked --quiet` (1 passed)
- `cargo fmt --all -- --check`, `git diff --check`

The normal message/lifetime regression is process-backed over local HTTP. The
malformed envelope is tested at the runtime dispatch boundary, not through a
page-originated process path, because normal page serialization cannot emit
malformed clone data. Remote CI, full Client/WindowClient interfaces,
cross-platform behavior, and full Service Worker/WPT conformance remain open
issue #40 work.
