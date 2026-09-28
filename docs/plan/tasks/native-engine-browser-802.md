---
id: native-engine-browser-802
scope: glass-browser/worker-object-messageerror-recovery
status: contracted
depends-on: [native-engine-browser-801]
---

# Glass native-engine browser slice 802: Worker-object messageerror recovery

## Objective

Recover the page-side Dedicated `Worker` receiver when an incoming
host-delivered message cannot be structured-deserialized. Dispatch
`messageerror` at the Worker object instead of aborting the page event turn,
and preserve later valid Worker messages.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- HTML's [Worker interface](https://html.spec.whatwg.org/multipage/workers.html#worker)
  includes `MessageEventTarget`; the
  [MessageEventTarget mixin](https://html.spec.whatwg.org/multipage/web-messaging.html#messageeventtarget-mixin)
  defines `onmessageerror` alongside `onmessage`.
- Slice [799](native-engine-browser-799.md) covers messages received by worker
  globals but explicitly leaves parent-side Worker-object decoding open.
  Slice [801](native-engine-browser-801.md) covers host-delivered MessagePort
  events through the same clone decoder but distinct receiving endpoints.
- The page-side owner dispatches Dedicated Worker host messages through
  `__glassDispatchWorkerMessage(workerId, payload)`. Its current valid-message
  branch installs object URLs and decodes the envelope without recovery, then
  dispatches a plain event-shaped object to the Worker proxy.

## Contract

- Scope is page-side host delivery to a live Dedicated `Worker` proxy when the
  payload does not represent a worker script error. Keep the existing
  `payload.error` ErrorEvent path unchanged. Catch only failures thrown while
  decoding the incoming structured-clone envelope; sender serialization,
  host validation, object-URL installation, listener exceptions, and worker
  script errors retain their existing behavior.
- A clone-decode failure dispatches a `MessageEvent` named `messageerror` at
  the receiving Worker object and never dispatches `message`. Set `data` to
  `null`, `ports` to an empty array, `source` to `null`, and preserve the
  payload's serialized `origin`. Set `target` and `currentTarget` to the
  Worker during callbacks and reset dispatch state afterward. Do not expose
  partial decoded values or transferred ports.
- Worker objects expose `onmessageerror` and accept `messageerror` listeners
  through their existing add/remove-event API. Dispatch the property handler
  and registered listeners using the current Worker event-dispatch policy;
  callback failures remain isolated as they are for `message` callbacks.
- Install object-URL transfers only after successful envelope decoding, so a
  failed clone does not publish URL state. Malformed-envelope test input is
  transfer-free; rollback of ports, buffers, URLs, or other transferred
  resources is separate work.
- Add process-backed local-HTTP coverage by injecting a bounded malformed
  envelope through the existing internal page dispatcher, checking both the
  handler and listener `messageerror` path, absence of a malformed `message`,
  and a later ordinary Worker message round trip. This internal fixture does
  not add a public corruption hook or imply sender-side web content can bypass
  serialization.
- SharedWorker port events, Service Worker client events, worker-global
  events, transferred-resource rollback, generic EventTarget/Web IDL
  conformance, full WPT, remote CI, and cross-platform certification remain
  outside this slice and issue #40 stays open.

## Boundaries and tradeoffs

- This is receiver recovery and does not convert synchronous sender-side
  serialization failures into asynchronous `messageerror` notifications.
- The existing worker-proxy callback ordering and exception-isolation policy
  are preserved; this slice does not certify full DOM EventTarget ordering or
  event-handler Web IDL reflection/conversion.
- The test uses a process-backed HTTP page and a real Dedicated Worker for
  the later valid path, but injects malformed internal clone data at the
  dispatch boundary because normal senders cannot serialize that envelope.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`

## Verification

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- The exact process-backed HTTP regression added for Worker-object
  `messageerror` delivery and later valid Worker messages.
- `cargo fmt --all -- --check`, `git diff --check`, and the focused repository
  documentation gates.

Record results and exclusions here after implementation. Remote CI is not
implied by local verification.
