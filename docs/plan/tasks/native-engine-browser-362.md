# Native startup Service Worker fetch suspension (362)

```yaml
id: native-engine-browser-362
scope: native-engine/service-worker-startup-fetch-suspension
status: done
depends-on:
  - native-engine-browser-361
```

## Objective

Allow a restored Service Worker to suspend the configured initial navigation
while awaiting `clients.openWindow()`. Native initialization must retain the
content-process continuation, enter the running state with the browser effect
queued, and let the normal browser-owned scheduler finish the navigation.

## Delivered behavior

- `initialize_async()` no longer tears down or rejects when the initial native
  load returns a bounded Service Worker fetch suspension.
- The runtime worker starts and the engine enters the running lifecycle while
  the pending open-window request remains available to the browser backend.
- The existing open-window effect creates the parked native target, resolves
  the restored worker Promise, resumes the retained initial request, and
  commits the Service Worker response through the existing native path.
- A restart-backed HTTP witness registers and persists the worker in one
  session, then boots a second native session directly at the intercepted
  initial URL and verifies the resumed document and parked target.

## Contract and tradeoffs

Initialization may return successfully before the document is committed only
for this bounded browser-owned effect handoff. The native backend immediately
drains the queued effect during its initialization dispatch; direct engine
callers must use the owning backend to resolve browser target effects. The
runtime worker is started before the first document commit so the continuation
can use the same running-state and recovery rules as later navigation.

The continuation remains bounded by the existing Service Worker and browser
effect limits. Registration arbitration, durable live-client leases, richer
transferables, complete task-source conformance, recovery/cancellation
certification, and final Core Web Profile promotion remain separate issue #40
gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-362.md`

## Verification

The following checks passed locally against the current checkout:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --lib --locked`
- `cargo test -p glass-browser --test native_engine native_runtime_restores_initial_service_worker_fetch_suspension --locked -- --nocapture` — 1 passed
- `cargo test -p glass-browser --test native_engine native_runtime_service_worker_ --locked -- --nocapture` — 3 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
