---
id: native-engine-browser-806
scope: glass-browser/worker-message-port-bridge-close
status: done
depends-on: [native-engine-browser-805]
---

# Glass native-engine browser slice 806: Worker MessagePort bridge close

## Objective

Make explicit `MessagePort.close()` work across the page-to-Dedicated/Shared
Worker bridge: retire the host route, stop messages on the disconnected
channel, and deliver one generic `close` Event to the still-open endpoint in
the other realm.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML Standard disentangle algorithm](https://html.spec.whatwg.org/multipage/web-messaging.html#message-ports)
  breaks the symmetric association and fires `close` at the other endpoint.
  MessagePort exposes `onclose` in addition to EventTarget listeners
  ([close steps and handlers](https://html.spec.whatwg.org/multipage/web-messaging.html#message-ports)).
- Slice [805](native-engine-browser-805.md) implements this for local
  MessageChannel pairs. A page/worker bridge has no local peer pointer: its
  opaque key is routed by `NativeWorkerRegistry.message_port_routes`, and
  pending cross-realm messages are held separately.
- Worker-originated MessagePort traffic and close records share the bounded
  `NativeMessagePortPageMessage` queue; page-originated controls use
  `NativeScriptCommand`. Slice 806 adds `MessagePortClose` and a close-event
  record so either owner can retire the Rust route and notify its peer.
- Service Worker MessagePort routes are owned by a separate registry and are
  deliberately outside this slice. They require their own route-owner and
  client-event audit.

## Contract

- Scope is page-to-Dedicated/Shared-Worker MessagePort bridges managed by
  `NativeWorkerRegistry`. Do not route a close command through a different
  owner or let an unknown key delete unrelated state.
- Add a bounded internal `MessagePortClose` command carrying the bridge key
  and the optional worker identity. Validate the bridge key, command origin,
  and route owner before changing state. A page may close a registered bridge
  endpoint for its own page; a Worker may close only a route owned by that
  worker. Repeated close of an already retired key is harmless.
- Closing either side marks only the initiating endpoint closed and removes
  its local ID/bridge registration. Retire the matching Rust route and purge
  queued host messages/commands for that key. The opposite realm's endpoint
  remains an open but disentangled `MessagePort`; remove its bridge key and
  dispatch exactly one generic `Event` named `close` through the existing
  `onclose`/listener path. Do not expose the host control command as a page
  event or a `message` event.
- Preserve the existing local-pair behavior from Slice 805. Keep unrelated
  bridge keys and routes live. After route retirement, no new `message` may
  cross the closed channel in either direction; a close event must not be
  duplicated by a repeated close or delayed stale host record.
- Use the existing serialized page/worker turn and task pump. Keep close
  records in the existing bounded MessagePort event queue; do not add a side
  queue. Reserve one queue slot for every live route, enforcing
  `live_routes + queued_events <= MAX_NATIVE_WORKER_MESSAGES`. Retiring a
  route converts its reservation into the peer's close record, so queue
  pressure cannot silently retire a route without admitting its close event.
  Deliver already accepted events in their existing order where the current
  queue model permits, and document any remaining task-source ordering gap.
- Add process-backed HTTP coverage for page-initiated and Worker-initiated
  close across a real Dedicated Worker bridge. Verify one generic close Event
  at each surviving peer, initiator closed/peer open state, route removal,
  no post-close messages, and an unrelated channel still working. Include
  SharedWorker coverage if it can use the same path without broadening the
  test fixture; otherwise keep that exact omission visible here.
- Receiver decode-failure cleanup from Slice 804 may use this command only
  for routes owned by this registry. Service Worker route cleanup, other
  transferables, GC/document-destruction close, full EventTarget/Web IDL and
  task-source conformance, WPT, remote CI, and cross-platform certification
  remain open issue #40 work.

## Boundaries and tradeoffs

- Removing only a JavaScript bridge map entry is insufficient: the Rust route
  and pending queue would continue retaining an endpoint the page can no
  longer reach. Conversely, deleting by key without checking owner identity
  could let one Worker retire another Worker's channel.
- The peer stays open and keeps its object identity after receiving `close`;
  it is disconnected, not itself closed. This preserves the specified event
  target and the behavior added for local channels in Slice 805.
- This does not certify ServiceWorkerGlobalScope-to-client MessagePort close
  behavior; that registry must receive a separately verified owner-specific
  integration rather than a NativeWorkerRegistry fallback. A close command
  matching a Service Worker-owned route must return an explicit owner-specific
  error; it must never be applied to the NativeWorkerRegistry or silently
  discard the Service Worker route.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`

## Verification

- `cargo check -p glass-browser --test native_engine --locked --quiet` — exit
  0; the current target reports 68 `dead_code` warnings and no compile errors.
- Exact regression:
  `cargo test -p glass-browser --test native_engine native_content_process_message_port_decode_recovery_and_bridge_close_both_directions --locked --quiet -- --exact`
  — passed: 1 test, 858 filtered, 27.11 seconds. It exercises real HTTP
  content-process Dedicated Worker routes in both directions, close Event
  identity/state, queue purge, repeated close, post-close delivery stop, and
  unrelated-channel survival.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- Documentation truth check — 1,434 Markdown files, zero current-claim
  failures; depth check — 93 routed current guides and 19 substantive
  contracts; TUI inventory — 15 implementation keys and 63 doc markers;
  documentation coverage — 1,434 files, 346 full-product MCP tools (101
  browser-only), 17 examples, and 22 public modules. All passed.

SharedWorker process coverage was omitted because it requires extending this
fixture with another worker resource and connection path. The implementation
uses the same registry route owner, but this is not SharedWorker conformance
evidence. At the Slice 806 checkpoint, Service Worker close was explicitly
rejected by its separate owner; Slice 807 later implements that route. Remote
CI, WPT, and cross-platform certification were not run.

Remote CI is not implied by local verification.
