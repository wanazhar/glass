---
id: native-engine-browser-803
scope: glass-browser/service-worker-client-messageerror-recovery
status: contracted
depends-on: [native-engine-browser-802]
---

# Glass native-engine browser slice 803: Service Worker client messageerror

## Objective

Recover the page-side `ServiceWorkerContainer` receiver when a message sent by
a Service Worker cannot be structured-deserialized. Dispatch
`messageerror` with the sending worker's source/origin instead of aborting the
page event turn, then prove a later ordinary Service Worker client message
still arrives.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The current [Service Workers specification](https://www.w3.org/TR/service-workers/)
  defines `ServiceWorkerContainer` `messageerror` as a `MessageEvent`. Its
  Service Worker-to-client postMessage algorithm fires this event with the
  sender's origin and source if structured deserialization fails.
- Slices [800](native-engine-browser-800.md),
  [801](native-engine-browser-801.md), and
  [802](native-engine-browser-802.md) cover Service Worker-global,
  MessagePort, and Dedicated Worker-proxy receiving paths respectively. The
  page-side Service Worker client dispatcher remains separate.
- `__glassDispatchServiceWorkerClientMessage(payload)` currently installs
  object URLs, decodes the envelope without recovery, and dispatches a
  message-shaped object at `navigator.serviceWorker`.

## Contract

- Scope is page-side delivery through the existing
  `__glassDispatchServiceWorkerClientMessage` host dispatcher. Catch only
  structured-clone envelope decode failures. Preserve sender-side
  serialization, host validation, URL-installation, and listener error
  behavior.
- A decode failure dispatches one `MessageEvent` named `messageerror` at the
  current page's `ServiceWorkerContainer`, never a `message` event. Initialize
  `data` to `null`, `ports` to an empty array, `source` to the page's existing
  active `ServiceWorker` projection when available, and `origin` from that
  sending worker's serialized origin. Set target/currentTarget and dispatch
  phase for callbacks, then reset currentTarget/phase. Do not expose partial
  clone data or ports.
- Expose a reflected-null `onmessageerror` handler slot on the container and
  invoke it along with registered messageerror listeners through its existing
  dispatch behavior. Successful client messages use the same `MessageEvent`
  interface and source/origin derivation.
- Install object-URL transfers only after successful envelope decoding. The
  malformed fixture is transfer-free; rollback of constructed ports, buffers,
  URLs, or other transferred resources remains separate work.
- Add a process-backed local-HTTP regression with an activated Service
  Worker. Inject one bounded malformed envelope through the existing internal
  page dispatcher, verify `MessageEvent` identity/fields/source/origin and
  absence of a `message`, then send a real Worker-to-client message through
  `Client.postMessage()` and verify recovery. Do not add a public corruption
  hook.
- Service Worker-global `ExtendableMessageEvent`, other receiver kinds,
  transfer rollback, generic EventTarget/Web IDL, full WPT, remote CI, and
  cross-platform certification remain separate gates; issue #40 stays open.

## Boundaries and tradeoffs

- This is a receive-side recovery change. Sender-side `postMessage()` clone
  errors remain synchronous and are not converted to `messageerror`.
- The Service Worker source projection remains the current page registry's
  active worker; browser-wide source-object identity and registration
  arbitration are not changed by this slice.
- The malformed fixture exercises the dispatch boundary because normal
  senders cannot emit malformed internal clone envelopes. It is not evidence
  that page content can bypass serialization checks.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`

## Verification

- `cargo check -p glass-browser --test native_engine --locked --quiet`
- The exact process-backed HTTP regression added for ServiceWorkerContainer
  `messageerror` and later client-message recovery.
- `cargo fmt --all -- --check`, `git diff --check`, and the focused repository
  documentation gates.

Record results and exclusions here after implementation. Remote CI is not
implied by local verification.
