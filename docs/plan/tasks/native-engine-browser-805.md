---
id: native-engine-browser-805
scope: glass-browser/local-message-port-close-event
status: contracted
depends-on: [native-engine-browser-804]
---

# Glass native-engine browser slice 805: local MessagePort close events

## Objective

Implement the same-realm `MessagePort.close()` disentangling behavior for
native `MessageChannel` pairs, including the required `onclose` handler and a
single `close` event at the still-open peer.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML Standard's disentangle steps](https://html.spec.whatwg.org/multipage/web-messaging.html#message-ports)
  break the symmetric peer relationship and fire `close` at the other port.
  The `MessagePort.close()` steps mark the initiating port detached and
  disentangle it. `MessagePort` also requires an `onclose` event-handler
  attribute ([close steps and handlers](https://html.spec.whatwg.org/multipage/web-messaging.html#message-ports)).
- The current native `close()` marks one JavaScript object closed, clears its
  queue, and removes its registry entry. It does not detach the peer or notify
  it, and the native handler-slot installer exposes only `onmessage` and
  `onmessageerror`.
- Slice [804](native-engine-browser-804.md) handles provisional transferred
  bridge proxies on decode failure. It does not close the peer or retire
  Rust-owned routes; cross-realm bridge teardown remains separate.

## Contract

- Scope is a same-realm, entangled `MessageChannel` pair created by the native
  implementation. Do not alter page/worker bridge routing in this slice.
- `onclose` is present on each native MessagePort as a nullable handler slot.
  Assigning or clearing it does not start the message queue. Existing
  `addEventListener("close", ...)` and `removeEventListener()` registration
  paths continue to work.
- Calling `close()` on an open entangled port marks only the initiating port
  closed, clears its queued deliveries, removes its own registry entry, and
  breaks both peer references. The other port remains open but disconnected.
  Fire exactly one generic `Event` named `close` at that peer, with the peer as
  target/currentTarget during callbacks and normal post-dispatch state reset.
  Do not fire `close` at the initiating port.
- Closing an already closed or unentangled port is idempotent and emits no
  additional event. Messages posted by the now-disconnected peer do not reach
  the closed port. Unrelated MessageChannels remain unaffected.
- Add the close assertions to a process-backed local-HTTP regression using
  the existing native MessageChannel fixture. Verify event type/interface,
  handler and listener delivery, callback `this`, target/phase, one-shot
  delivery, and that the disconnected pair does not deliver later messages.
- Host bridge route retirement and cross-realm close notification,
  document-destruction/garbage-collection disentangling, complete EventTarget
  ordering/EventHandler Web IDL semantics, task-source fairness, full WPT,
  remote CI, and cross-platform certification remain separate issue #40 work.

## Boundaries and tradeoffs

- The initiating port is marked closed; the peer is merely disentangled and
  stays open. Treating both objects as closed would prevent the peer from
  retaining its normal object identity and would not match the specified
  close event target.
- This slice reuses the current MessagePort event-dispatch policy. It verifies
  close delivery and dispatch state, but does not claim complete DOM
  listener/property-handler registration ordering.
- The current native runtime uses explicit close, not a JavaScript garbage
  collector finalization hook. GC-driven remote close remains a separate
  lifecycle contract.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`

## Verification

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- Exact process-backed HTTP regression for same-realm close event delivery
  and pair disconnection.
- `cargo fmt --all -- --check`, `git diff --check`, and focused repository
  documentation gates.

Record exact results and exclusions here after implementation. Remote CI is
not implied by local verification.
