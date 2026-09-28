---
id: native-engine-browser-801
scope: glass-browser/message-port-deserialization-error-recovery
status: completed locally
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

## Implementation and verification

The shared native MessagePort bootstrap now catches only failures from
`glassMessageDecodeEnvelope()` in both local-ID and bridge host dispatch. It
enqueues a bounded `messageerror` record through the receiving port's existing
queue, constructs a `MessageEvent` with null data and no ports, and leaves the
port open. Object-URL transfers are installed only after successful clone
decoding, so a failed envelope cannot publish them. Sender-side serialization,
host validation, object-URL installation, listener dispatch, and queue-limit
errors remain outside the catch.

The process-backed HTTP regression
`native_content_process_message_port_decode_failure_dispatches_messageerror_and_recovers`
transfers a port from a Dedicated Worker to its page, then injects a
transfer-free malformed envelope through both the local-ID and bridge
dispatchers. It verifies `MessageEvent` identity, event type/target/current
target, null data, empty ports, no partial `message`, reset dispatch state,
and that both local and cross-realm ports remain open and carry a later valid
message. The cross-realm valid message round-trips through the worker.

Passed locally:

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine native_content_process_message_port_decode_failure_dispatches_messageerror_and_recovers --locked --quiet -- --exact` (1 passed)
- `cargo test -p glass-browser --test native_engine native_content_process_transfers_message_ports_between_page_and_worker_realms --locked --quiet -- --exact` (1 passed)
- `cargo fmt --all -- --check`, `git diff --check`
- Release-truth and documentation-depth checks (see current issue #40 checkout evidence).

The malformed fixture has no transferred ports or object URLs. Transfer
rollback, other `messageerror` sources, remote CI, complete EventTarget/Web IDL
and WPT behavior, and cross-platform certification remain open issue #40 work.
