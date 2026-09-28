---
id: native-engine-browser-806
scope: glass-browser/worker-message-port-bridge-close
status: contracted
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
- Worker-originated MessagePort traffic is queued as
  `NativeMessagePortPageMessage`; page-originated traffic is represented by
  `NativeScriptCommand::MessagePortPostMessage`. There is no close command or
  close-event host record yet, so local map deletion does not retire the Rust
  route or notify the other realm.
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
- Use the existing serialized page/worker turn and task pump. Bound any added
  close-event queue by the current MessagePort message limit; do not introduce
  an unbounded side queue. Deliver already accepted events in their existing
  order where the current queue model permits, and document any remaining
  task-source ordering gap.
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
  integration rather than a NativeWorkerRegistry fallback.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`

## Verification

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- Exact process-backed HTTP regression for both close directions, route
  retirement, independent-channel survival, and post-close delivery stops.
- `cargo fmt --all -- --check`, `git diff --check`, and focused repository
  documentation gates.

Record exact results and exclusions here after implementation. Remote CI is
not implied by local verification.
