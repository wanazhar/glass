---
id: native-engine-browser-804
scope: glass-browser/failed-envelope-provisional-message-ports
status: completed locally
depends-on: [native-engine-browser-803]
---

# Glass native-engine browser slice 804: failed-envelope port cleanup

## Objective

Prevent failed structured-clone deserialization from permanently rooting
receiver-side `MessagePort` bridge projections that were created only for the
failed delivery. Preserve ports that existed before the decode attempt and keep
the actual receiving endpoint usable for later messages.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML Standard's `StructuredDeserializeWithTransfer`](https://html.spec.whatwg.org/multipage/structured-data.html#structureddeserializewithtransfer)
  receives transferred values before deserializing the serialized object
  graph, and returns its transferred-values list only after that graph
  deserializes successfully. `MessagePort` transfer-receiving steps move its
  queued tasks and entangle the receiving endpoint
  ([MessagePort transfer steps](https://html.spec.whatwg.org/multipage/web-messaging.html#message-ports)).
- The messaging algorithms catch a receiver deserialization failure and fire
  `messageerror` instead of `message`; they do not restore the already
  transferred object to the sending realm
  ([Window postMessage](https://html.spec.whatwg.org/multipage/web-messaging.html#window-post-message-steps),
  [MessagePort postMessage](https://html.spec.whatwg.org/multipage/web-messaging.html#message-port-post-message-steps)).
- `glassMessageDecodeEnvelope()` currently materializes each transfer-port
  descriptor through `glassMessageMakeBridgePort()` before decoding the graph.
  New proxies are added to strong `glassMessagePortRegistry` and
  `glassMessageBridgeRegistry` maps; a later decode exception leaves them
  registered even though no event exposes them.
- Slices [801](native-engine-browser-801.md),
  [802](native-engine-browser-802.md), and
  [803](native-engine-browser-803.md) recover the receiving event turn, but
  deliberately use transfer-free malformed fixtures. This slice adds the
  missing provisional-port lifetime case.

## Contract

- Scope is the shared receiving `glassMessageDecodeEnvelope()` path and its
  provisional JavaScript bridge-port projections. Preserve each caller's
  existing error boundary, event target, task/queue behavior, and
  `messageerror` handling.
- Track which bridge proxies are newly created for the current envelope
  attempt. If graph decoding fails, mark only those provisional proxies
  unusable, clear their local queues, and remove their ID/key entries from the
  current realm registries. Removal must be conditional on the registry still
  mapping that key/ID to the same object. A proxy that was already registered
  before this attempt must remain open and registered.
- Preserve and rethrow the original decode failure after cleanup. Do not
  dispatch a partial `message`, expose partially decoded values or ports, or
  alter the existing `messageerror` event. Install transferred object URLs
  only after successful graph decoding, as already required by Slices 801–803.
- Do not roll a transfer back into the sender realm. Sender-side transfer
  commitment precedes asynchronous receiver decoding; the receiving transfer
  values are not script-exposed when the graph fails. This slice reclaims only
  provisional JS bridge projections rooted by the native decoder.
- Add process-backed HTTP regression coverage with one malformed graph and a
  transfer-port descriptor. Verify the failure event exposes no transferred
  port, provisional registry entries are gone, any preexisting bridge entry is
  preserved, and a later valid message on the receiving endpoint still works.
  Injection uses existing internal dispatchers only; add no public corruption
  hook.
- Host-side bridge-route retirement, remote-peer `close` notification,
  cleanup of partially received non-port transferables, generic EventTarget/
  Web IDL, complete MessagePort/WPT conformance, remote CI, and cross-platform
  certification remain separate issue #40 gates.

## Boundaries and tradeoffs

- The standard explicitly requires `messageerror` recovery and specifies
  receiving transfer steps, but it does not provide a rollback operation that
  resurrects the source-side object after receiver graph decoding fails. The
  implementation must not invent one.
- Strong JavaScript registries make a discarded proxy live longer than an
  ordinary unreachable JavaScript value. Removing only newly-created entries
  avoids retaining such proxies without invalidating a previously usable
  endpoint.
- This slice does not claim that Rust-owned routes or the opposite realm's
  endpoint are retired. Those ownership and `close`-event semantics must be
  audited and handled separately rather than hidden behind local map cleanup.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`

## Implementation and verification

`glassMessageDecodeEnvelope()` now records bridge proxies that were absent (or
closed) before the current decode attempt. If graph decoding throws, it marks
only those new proxies closed, clears their local queues, and removes their
port-ID and bridge-key entries only when the registries still map those keys
to the same objects. The original exception is rethrown. Previously open
proxies remain registered; successful decoding is unchanged.

The process-backed HTTP regression
`native_content_process_message_port_decode_failure_dispatches_messageerror_and_recovers`
now includes both a fresh transfer descriptor and a preexisting bridge
registration in its malformed envelopes. It verifies the provisional entry
is removed, the preexisting endpoint remains open, registry sizes return to
their pre-failure values, each receiving endpoint reports `messageerror`
without partial ports, and both endpoints still carry later valid messages.

Passed locally:

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine native_content_process_message_port_decode_failure_dispatches_messageerror_and_recovers --locked --quiet -- --exact` (1 passed, 858 filtered; 22.32 seconds)
- `cargo fmt --all -- --check` and `git diff --check`
- `python3 scripts/check-release-documentation.py --require-previous-version` (1,432 Markdown documents; zero current-claim failures)
- `python3 scripts/check-documentation-depth.py` (93 current guides routed/audited; 19 substantive contracts)

These are focused local results only. Host-side route retirement, remote-peer
`close` notification, cleanup of partially received non-port transferables,
complete MessagePort/EventTarget/Web IDL and WPT conformance, remote CI, and
cross-platform certification remain open. Local verification does not imply
remote CI.
