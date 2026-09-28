---
id: native-engine-browser-801
scope: glass-browser/message-port-deserialization-error-recovery
status: contracted
depends-on: [native-engine-browser-800]
---

# Glass native-engine browser slice 801: MessagePort deserialization recovery

## Objective

When an incoming host-delivered `MessagePort` message cannot be
structured-deserialized in its receiving realm, dispatch `messageerror` at
that port instead of aborting the host delivery turn. Preserve the port and
its delivery queue so a later valid message still works.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The HTML Standard's [MessagePort message-delivery
  steps](https://html.spec.whatwg.org/multipage/web-messaging.html#message-ports)
  catch a `StructuredDeserializeWithTransfer` failure and fire a `messageerror`
  `MessageEvent` at the receiving port rather than firing `message`.
- Slice [799](native-engine-browser-799.md) applies corresponding recovery to
  direct worker-global messages. Slice
  [800](native-engine-browser-800.md) applies the distinct
  `ExtendableMessageEvent` contract to Service Worker global messages.
- The native MessagePort bootstrap receives host deliveries through
  `__glassDispatchMessagePortById` and
  `__glassDispatchMessagePortByBridge`; both currently let clone-envelope
  decoding failures escape the page/worker owner.

## Contract

- Scope is host-delivered MessagePort events entering the shared native
  MessagePort bootstrap through the local-ID or bridge dispatcher. Catch only
  errors thrown while decoding the incoming structured-clone envelope.
  Sender-side serialization/transfer-list errors, host validation failures,
  object-URL installation errors, and listener dispatch failures retain their
  existing behavior.
- A decode failure dispatches one `MessageEvent` named `messageerror` at the
  receiving port, never a `message` event. Its data is `null` and its ports
  list is empty; no partially decoded data or transferred port is exposed.
  `target` and `currentTarget` identify the receiving port during callbacks,
  and dispatch state is reset afterward. Preserve the existing port queue and
  start behavior; the port is not closed or detached by the failure.
- Do not install object-URL transfers until the clone envelope decodes
  successfully. This prevents a failed decode from publishing URL state. This
  slice's malformed-envelope coverage is transfer-free; rollback of partially
  constructed transferred ports or other transferables remains separate work.
- The messageerror record follows the receiving port's existing queued
  delivery path and bounds. A later valid delivery to that port must still
  dispatch normally. No new public corruption hook is introduced; malformed
  input is injected only through the existing internal dispatcher in a
  process-backed local-HTTP regression.
- Add process-backed coverage for a transfer-free malformed clone envelope,
  event type/interface/target/data/ports, absence of a `message` event, and a
  later valid message. Remote CI, all message source variants, transfer
  rollback, BroadcastChannel and Worker-proxy decode failures, complete
  EventTarget/Web IDL behavior, and full WPT conformance remain outside this
  slice and issue #40 stays open.

## Boundaries and tradeoffs

- This is receive-side recovery only. Catching sender-side errors would
  incorrectly turn synchronous `DataCloneError` failures into asynchronous
  receiver events.
- The transfer-free malformed test proves that a clone failure no longer
  kills this delivery path; it does not establish atomic rollback for
  transferred ports, buffers, or object URLs.
- The native MessagePort implementation continues to use its current queue
  scheduling model. This task does not claim complete HTML task-source or
  MessageEvent Web IDL conformance.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`

## Verification

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- The exact process-backed HTTP regression added for the malformed-envelope
  and recovery behavior.
- `cargo fmt --all -- --check`, `git diff --check`, and the focused repository
  documentation gates.

Record results and exclusions here after implementation. Remote CI is not
implied by local verification.
