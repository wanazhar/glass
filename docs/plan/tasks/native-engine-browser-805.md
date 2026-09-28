---
id: native-engine-browser-805
scope: glass-browser/local-message-port-close-event
status: completed locally
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

## Implementation and verification

Native MessagePorts now expose `onclose` without starting their message
queues. Closing a local entangled endpoint marks only that endpoint closed,
clears its queue, removes its registry identity, and severs both peer
references. If the peer still points back and remains open, the runtime fires
one generic `Event` named `close` at that peer through the existing handler
and listener dispatcher. Repeated close is idempotent. Bridge endpoints do
not gain host-route or remote-close behavior in this slice.

The process-backed HTTP regression
`native_content_process_message_port_decode_failure_dispatches_messageerror_and_recovers`
now verifies both `onclose` and listener callbacks receive the same Event
with the peer as `this`, target, and currentTarget during dispatch. It also
checks the Event type/state, no event on the initiating port, one-shot close,
peer openness, symmetric disconnection, and no later message delivery across
the pair.

Passed locally:

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine native_content_process_message_port_decode_failure_dispatches_messageerror_and_recovers --locked --quiet -- --exact` (1 passed, 858 filtered; 21.48 seconds)
- `cargo fmt --all -- --check` and `git diff --check`
- `python3 scripts/check-release-documentation.py --require-previous-version` (1,433 Markdown documents; zero current-claim failures)
- `python3 scripts/check-documentation-depth.py` (93 current guides routed/audited; 19 substantive contracts)

These are focused local results only. Cross-realm bridge route retirement
and close delivery, document-destruction/GC close behavior, complete
EventTarget/EventHandler Web IDL ordering, task-source fairness, full WPT,
remote CI, and cross-platform certification remain open. Local verification
does not imply remote CI.
