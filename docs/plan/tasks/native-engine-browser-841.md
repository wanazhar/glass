id: native-engine-browser-841
scope: glass-browser/native-engine/async-worker-effect-owner-pump
status: in-progress
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
- `.github/workflows/ci.yml`
- `docs/plan/native-engine-browser-profile.md`
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
- Linux runs the complete native-engine feature suite; macOS and Windows run
  the socket-free `asynchronous_effect`, `async_effect`, and
  `dropping_runtime_backend` test filters in the browser-free platform matrix.
  Exact-source green CI evidence is required before closure.
- `cargo check -p glass-browser --features native-engine --lib --tests --locked -q`
- `cargo fmt --all -- --check`
- `git diff --check`

## Current Evidence

- The engine boundary accepts a notification only when its nonzero sequence
  and `(context_id, frame_id)` match that exact `NativeEngine`; it advances the
  owner through one script turn and returns DOM effects and frame-script
  requests.
- The native runtime owns the wake task. It drains child notifications across
  live target/frame owners, queues them within the maximum-live-owner bound,
  routes each to the exact context/frame, then processes page events, frame
  scripts, browser effects, and ServiceWorker client synchronization. Pump
  failures are retained for later operations. Shutdown has a dedicated
  cancellation signal, separate from event wakeups, which drops an in-flight
  owner-turn future before joining the task. The future-drop regression also
  verifies release of a lock held by the suspended test operation; the
  existing cancelled-dialog identity-cleanup regression passes. Dropping the
  runtime backend also signals
  the pump while an internal strong reference is live; its task-join regression
  passes. Runtime-level blocked-owner shutdown is not yet proven. No
  Chromium/CDP fallback is involved.
- Socket-free engine tests cover owner acceptance/rejection. Backend fixture
  regressions cover parked-target dispatch and reject a frame paired with the
  wrong context. The canonical runtime lifecycle regression covers startup and
  close. The scoped native-engine check passes with existing dead-code warnings
  from the superseded HTML parser. Both
  `async_effect_owner_wait_does_not_hold_target_registry` and
  `parent_frame_event_wait_does_not_hold_target_registry` pass: each holds an
  owner lock while its pump/routing phase waits, confirms the target registry
  remains available, then releases the owner and confirms the operation
  settles. The frame-event regression directly exercises the validated-route
  phase because local `fixture://` origins are intentionally opaque; it proves
  lock liveness only, not successful event projection. These tests do not prove
  process-backed timer/lifetime delivery. The focused
  `service_worker_sync_owner_wait_does_not_hold_target_registry` fixture test
  also passes: it holds a parked owner, confirms synchronization remains pending
  while the target registry is available, changes active-frame topology,
  releases the owner, and verifies synchronization discards the stale snapshot,
  retries, and completes with the new topology.
  `service_worker_sync_retries_topology_drift_during_publication` also pauses
  after the first owner receives its client list, changes active-frame topology,
  then verifies the in-flight sync retries and converges without overwriting
  that change.
- CI runs the socket-free `async_effect` contract tests on macOS and Windows,
  covering the `asynchronous_effect`, `async_effect`, and
  `dropping_runtime_backend` filters, while the existing Linux native-engine
  job runs the full feature suite. Remote green evidence for the current source
  has not yet been recorded.
- **Current lock/publication boundary:** the pump and parent/frame routes
  snapshot stable owner handles under the target registry, then wait after
  releasing it; owner draining remains sequential. ServiceWorker synchronization
  now serializes sync invocations with a dedicated async gate, reconciles a
  cloned target/frame snapshot off-lock, validates target and owner identities
  before commit, and returns an explicit selection error after three topology
  conflicts. It revalidates before lease persistence and again after sequential
  async client replacement; detected drift retries the full synchronization
  against a new snapshot, bounded by the same three-conflict limit. Lease
  persistence remains synchronous under the registry. A stale list may be
  visible to owners updated before drift is detected, so atomic multi-owner
  publication is not guaranteed. Owner draining remains sequential; a blocked
  owner still delays this synchronization cascade.
- Process-backed delivery across separate targets and frames, event ordering,
  stale-owner teardown, cascade coverage, queue overflow, shutdown races,
  selected ServiceWorker/DedicatedWorker/SharedWorker/MessagePort/event-loop WPT
  cases, and green exact-source native-only cross-platform CI results remain
  unverified. This task is not complete.
- Issue #40 remains open.
