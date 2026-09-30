id: native-engine-browser-841
scope: glass-browser/native-engine/async-worker-effect-owner-pump
status: ready
depends-on: [native-engine-browser-840]
---

# Glass native-engine browser slice 841: asynchronous worker-effect owner pump

## Objective

Route accepted content-worker effect notifications to their exact live target
and frame without waiting for a user/browser request. Execute the owner's
normal script turn and pass resulting browser-facing effects through the
existing bounded cascade scheduler.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- [Slice 840 transport](native-engine-browser-840.md)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [HTML event loops](https://html.spec.whatwg.org/multipage/webappapis.html#event-loops)
- [Service Workers](https://www.w3.org/TR/service-workers/)

## Contract

- Start the pump with the native runtime and stop it on shutdown. It must not
  keep the backend alive after shutdown or lose errors from the owner loop.
- Route each notification by its validated context/frame identity. Never infer
  ownership from the currently selected target. Stale target/frame behavior
  follows the documented browser lifecycle and cannot redirect to a survivor.
- Run page callbacks in the owning realm and process produced popup, navigation,
  close, postMessage, MessagePort, ServiceWorker, cookie, and shared-worker
  effects through `process_pending_browser_effects` and its bounded cascade.
- The pump waits without holding page/target operation locks. A busy or blocked
  worker must not serialize unrelated targets; queue backpressure remains
  bounded and preserves event order.
- Shutdown, target/frame closure, child failure, queue overflow, and protocol
  failure release readers/waiters and surface explicit errors. Native delivery
  must not start Chromium or CDP.

## Path

- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/backend_factory.rs`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-841.md`
- `CHANGELOG.md`

## Verification

- Process-backed tests prove timer and lifetime effects reach the correct page
  while no external browser request is in flight; include separate targets and
  frames, event order, stale owners, cascades, queue bounds, and shutdown.
- Relevant ServiceWorker, DedicatedWorker, SharedWorker, MessagePort, and HTML
  event-loop WPT cases, with selected cases and deviations recorded.
- `cargo check -p glass-browser --features native-engine --lib --tests --locked -q`
- `cargo fmt --all -- --check`
- `git diff --check`

## Current Evidence

- Pending. Slice 840 must provide the validated event-ready transport first.
- Issue #40 remains open.
