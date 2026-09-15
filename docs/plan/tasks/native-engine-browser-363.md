# Native browser-wide Service Worker registration synchronization (363)

```yaml
id: native-engine-browser-363
scope: native-engine/service-worker-registration-synchronization
status: done
depends-on:
  - native-engine-browser-362
```

## Objective

Keep target and frame content processes synchronized with the persisted native
browser profile so a Service Worker registration or unregistration in one
running context is visible to other already-running contexts before their next
operation.

## Delivered behavior

- A bounded content-process synchronization command reloads the persistent
  Service Worker registration profiles before normal operations.
- The live registry reconciles replaced or removed active and waiting workers,
  removes their stale message, `openWindow()`, and fetch routes, and clears
  client scope state that no longer exists in the profile.
- Profile-backed registration state is projected into a document before a
  worker realm is lazily restored, so a late registration is observable in an
  already-running target.
- Sessions without persistent storage retain their in-memory registrations;
  the synchronization path is skipped when no storage profile exists.
- Service Worker scope matching respects exact path boundaries, including root
  and trailing-slash scopes; `/app` does not match `/application`.
- The backend synchronizes the active target, parked targets, and their frame
  owners before normal native operations.
- The integration witness covers a target created before registration, late
  registration visibility, controlled interception, cross-target unregister,
  and network fallback after removal.

## Contract and tradeoffs

Persistent native target operations incur one bounded IPC round trip and
profile read before each normal non-initialization operation. No-storage
sessions skip that work and keep their existing in-memory ownership model.

Profile-backed state may be exposed before the worker realm is instantiated;
navigation or another worker operation then restores the realm through the
existing startup path. This slice provides browser-wide synchronization within
one native backend and profile. Full registration arbitration across
independent Glass instances, durable live-client leases, and complete
task-source conformance remain issue #40 gates.

When an external profile refresh observes unregister or replacement, pending
worker routes are removed so stale deliveries cannot reach a worker that is no
longer authoritative. The existing bounded worker and effect limits remain in
force.

## Touched paths

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-363.md`

## Verification

The following checks passed locally against the current checkout:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --lib --locked`
- `cargo test -p glass-browser --test native_engine native_runtime_propagates_service_worker_registration_to_running_target --locked -- --nocapture` — 1 passed
- `cargo test -p glass-browser --test native_engine service_worker --locked -- --nocapture` — 14 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
