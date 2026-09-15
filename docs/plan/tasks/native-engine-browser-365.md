# Native per-scope Service Worker registration persistence arbitration (365)

```yaml
id: native-engine-browser-365
scope: native-engine/service-worker-registration-persistence
status: done
depends-on:
  - native-engine-browser-364
```

## Objective

Preserve independent Service Worker registration updates when multiple native
content owners persist a shared profile from stale in-memory snapshots.

## Delivered behavior

- Each native Service Worker registry records the final registration profile or
  deletion tombstone for every scope changed by that owner.
- Profile persistence validates the bounded change map, locks the profile, reads
  the latest durable registration vector, applies only the changed scopes, and
  writes the merged vector atomically.
- An owner updating one scope no longer replaces unrelated registrations made
  by another owner after the first owner loaded its snapshot.
- Deletion tombstones remove only their recorded scope; a later registration of
  that scope replaces the tombstone in the owner journal.
- The content owner clears its journal only after the profile commit succeeds,
  so a failed write remains retryable and a no-storage owner does not retain
  irrelevant persistence state.

## Contract and tradeoffs

The existing profile lock serializes profile writes. Independent scope changes
are merged against the latest profile, while two owners changing the same scope
use last-writer-wins according to lock acquisition order. This is a bounded
per-scope persistence arbitration boundary; it does not claim full browser
event-order arbitration across registration algorithms, worker task sources,
or separate profile stores. Registration and tombstone inputs remain bounded
and fail closed when malformed.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-365.md`

## Verification

The following checks passed locally against the current checkout:

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --lib --locked browser::native_engine::javascript::storage_journal_tests::service_worker_registration_profile_writes_merge_scoped_changes -- --exact` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine --locked native_content_process_updates_service_worker_registration -- --exact` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine --locked native_runtime_service_worker -- --nocapture` — 4 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
