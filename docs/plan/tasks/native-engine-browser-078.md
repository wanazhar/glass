---
id: native-engine-browser-078
scope: glass-browser/native-engine/indexeddb
status: done
depends-on: [native-engine-browser-077]
---

# BE-08: bounded IndexedDB persistence and transactions

## Objective

Give local documents and sandboxed HTTP(S) content workers a useful,
origin-keyed IndexedDB surface that survives an explicit storage-profile
restart, while keeping the implementation bounded and owned by the existing
native runtime. This closes the previous IndexedDB denial for the supported
subset without claiming full browser storage parity.

## Contract

- The profile stores IndexedDB beside the existing versioned Web Storage,
  cookie, journal, and reader-lease state. State is keyed by the same storage
  key used by page Web Storage, so tuple origins share databases across
  navigation while opaque fixture documents retain document-scoped keys.
- Each origin may retain at most 16 databases, each database at most 128
  object stores, and each object store at most 128 records. Database/store
  names and keys use the bounded backend identifier limit; JSON record values
  are capped at 8 KiB and the full IndexedDB snapshot at 64 KiB.
- The shared QuickJS bootstrap exposes positive-version `indexedDB.open`,
  `deleteDatabase`, and `databases`, plus database connections, upgrade
  transactions, object-store creation/deletion, readonly/readwrite
  transactions, `abort`, and bounded `get`, `getAll`, `count`, `put`, `add`,
  `delete`, and `clear` requests. Key paths and auto-increment keys are
  supported for the bounded JSON value subset.
- Requests are ordered through the existing bounded microtask queue. An
  upgrade callback runs before the open success callback; transaction
  completion waits for requests queued by request callbacks. Invalid modes,
  versions, names, keys, values, missing stores, duplicate adds, and quota
  violations fail as typed JavaScript errors.
- The parent engine and content worker exchange a validated full IndexedDB
  snapshot in the existing protocol response. The parent remains the profile
  owner; the worker never opens the profile, event journal, lock, or reader
  lease files. Profile writes retain the existing lock and atomic replacement
  path.

## Deliberate boundary and tradeoffs

This is a bounded JSON structured-clone subset. It does not implement indexes,
cursors, key ranges, binary/blob/date/regexp/map/set values, live Web IDL
identity, quota-management APIs, version-change connection coordination, or
the browser's complete transaction scheduling semantics. Transactions are
realm-local and the current profile path carries the latest validated snapshot;
IndexedDB changes are not yet represented as independent journal deltas or
merged key-level cross-process transactions. A later storage-convergence slice
must add that authority before claiming multi-writer IndexedDB parity.

Page-setup script loading temporarily suppresses the timer pump and the engine
resets the deterministic timer clock at the page-operation boundary. This
prevents queued setup callbacks from consuming the first observable timer turn
while preserving explicit evaluation and interaction timer behavior.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

All checks below were run on the local-only `main` checkout. No push, remote CI,
release, tag, or registry-publication claim is made by this task.

- `cargo fmt --all` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_interval_reschedules_until_cleared -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine indexed_db_ -- --nocapture` — 2 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine storage -- --nocapture` — 12 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 371 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --all-targets -- -D warnings` — expected non-zero baseline of 32 documented pre-existing diagnostics; no new diagnostics after the explicit high-arity allowance for the expanded inline-script pipeline
