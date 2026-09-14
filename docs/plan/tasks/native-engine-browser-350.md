# Native Service Worker lifecycle observability (350)

```yaml
id: native-engine-browser-350
scope: native-engine/service-worker-lifecycle
status: done
depends-on: [native-engine-browser-349]
```

## Objective

Complete the bounded page-facing Service Worker lifecycle contract without
changing the existing Rust ownership boundary:

- publish the install/installed/activating/activated transition sequence for a
  successful registration or update;
- expose listener-bearing `ServiceWorker` and `ServiceWorkerRegistration`
  objects with `statechange` and `updatefound` delivery;
- preserve the previous active worker while an update transition is being
  observed, then mark it redundant after the replacement activates;
- make `ServiceWorkerRegistration.update()` resolve `undefined`, as required
  by its Promise contract, while retaining the updated registration object for
  later inspection; and
- remove an unregistered page registration from the native page-facing map
  after the host confirms the operation.

The native owner continues to settle lifecycle `waitUntil()` work before it
publishes a successful transition. Failed script loads or lifecycle work leave
the previous active worker untouched.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-342.md`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-350.md`

## Contract and tradeoffs

Lifecycle states are delivered as a bounded host-owned transition list. The
page runtime reuses the same worker object through installing, installed,
activating, and activated states so listeners observe the identity change; the
former active worker receives a final redundant transition after replacement.
The current native owner still performs a successful update atomically before
the page resolver runs, so this is observable lifecycle ordering rather than a
claim of a concurrently running browser task queue.

The slice does not yet implement a persistent waiting worker, exact
`skipWaiting()` activation arbitration, multi-client controller ownership, or
full browser task-source scheduling. Those remain explicit promotion work and
must not be inferred from the lifecycle witness.

## Verification

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_observes_service_worker_lifecycle_update --locked`
- `git diff --check`
- repository documentation coverage, depth, release-truth, and TUI shortcut
  validators

All evidence is local unless a later explicitly authorized push produces
remote CI evidence.

## Local evidence

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine service_worker --locked` — 6 passed
- documentation coverage, depth, release-truth, and TUI shortcut validators
- `git diff --check`
