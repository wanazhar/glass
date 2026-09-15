# Native durable Service Worker client leases (364)

```yaml
id: native-engine-browser-364
scope: native-engine/service-worker-client-leases
status: done
depends-on:
  - native-engine-browser-363
```

## Objective

Keep the browser-wide Service Worker client projection durable across
independent native sessions and content-process replacement, while removing
clients from orderly close and bounding residue left by a crashed owner.

## Delivered behavior

- A versioned, bounded `.clients` sidecar beside the native profile stores
  stable window/frame client state, an owner token, and a heartbeat timestamp.
- Lease reads and writes use the existing profile lock and atomic temporary-file
  replacement; malformed, oversized, duplicate, or unsupported records fail
  closed.
- Native engine construction loads surviving leases before content-process
  startup, so restored Service Worker realms receive the complete durable client
  projection on their first turn.
- Each native backend synchronization refreshes active, parked, and frame-owned
  local leases, prunes expired records, and merges external clients before the
  next normal operation.
- Orderly engine close and `Drop` remove only the matching client ID and owner
  token. A replacement owner can take over a stable client ID without an old
  owner deleting the replacement lease.
- The integration witness covers two independent native sessions enumerating
  one another through `clients.matchAll()`, followed by close-time removal.
- The deterministic unit witness covers stale-heartbeat pruning and
  owner-fenced release.

## Contract and tradeoffs

Leases are refreshed at native operation boundaries and expire after a bounded
15-minute idle window. This avoids a background heartbeat thread and keeps
steady-state resource use low, but an owner that remains completely idle beyond
the window is treated as crashed by another session. A future autonomous
browser event loop can replace this bounded policy when cross-source task
ownership is complete.

The sidecar is separate from the Web Storage profile so client liveness does
not rewrite application data. Stable client IDs remain derived from browser
context/frame identity; owner tokens fence cleanup, while a newer owner may
replace an abandoned duplicate identity. Full multi-instance registration
arbitration, background task/event parity, and final Core Web Profile
certification remain issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-364.md`

## Verification

The following checks passed locally against the current checkout:

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --lib service_worker_client_leases_prune_stale_and_fence_release --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_runtime_service_worker_client_leases_cross_sessions_and_close --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine service_worker --locked -- --nocapture` — 15 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
