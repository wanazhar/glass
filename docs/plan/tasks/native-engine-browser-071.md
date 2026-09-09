---
id: native-engine-browser-071
scope: glass-browser/native-engine/process-backed-local-storage-event-delivery
status: done
depends-on: [native-engine-browser-070]
---

# BE-02/BE-03/BE-04/BE-07: process-backed local storage events

## Objective

Extend the bounded same-profile `localStorage` event contract from local
native documents to HTTP(S) documents hosted by separate sandboxed content
workers. A Glass process may own multiple concurrently alive
`NativeEngine` instances; the parent-side coordinator remains the source of
truth for fan-out, while each content worker owns its document and JavaScript
realm.

## Contract

- A content worker reports effective page `localStorage` changes from page
  load scripts, explicit script evaluation, typed event/action bridges, and
  lifecycle dispatch through a bounded `storage_events` response field.
- The parent validates event shape, scope, storage identity, URL, value sizes,
  and event count before applying local-storage changes and publishing them to
  other subscribers. `sessionStorage` descriptors are accepted for the
  worker's own state but are never broadcast by the parent coordinator.
- A receiving process-backed engine drains its profile subscriber queue at the
  next host operation, updates its parent Rust storage state, and sends a
  bounded typed `storage_events` command into its worker before the next page
  script, action, or navigation lifecycle operation.
- The receiving worker applies each event to its Rust-owned storage map and
  queues it for the persistent JavaScript realm. The next evaluation dispatches
  `StorageEvent` with `key`, `oldValue`, `newValue`, `url`, and `storageArea`;
  the event source document remains excluded.
- The source worker's storage state is persisted through the existing explicit
  profile path. The event source is excluded from coordinator fan-out, so it
  does not observe its own write.
- Worker IPC and parent queues remain bounded by the existing native effect,
  script, document, and frame limits. Invalid or oversized event payloads are
  typed native failures; they do not silently fall back to CDP.

## Ownership and sequence

```text
source worker realm
  -> bounded response storage_events
  -> parent engine validates/applies current state
  -> profile coordinator publishes to other live engines
  -> receiving parent drains/filter by local origin key
  -> receiving worker storage_events command
  -> receiving realm dispatches StorageEvent before next evaluation
```

The coordinator is deliberately parent-side and in-process. The content
workers do not open peer channels or share mutable state directly. This keeps
the sandbox boundary and single-owner document model intact while making the
observable event surface work for network pages.

## Deliberate boundary and tradeoffs

This slice covers two or more concurrently alive process-backed documents in
one Glass process sharing one explicit profile path. It does not claim:

- `sessionStorage` event routing by top-level browsing-context identity;
- coordination between independent Glass processes or concurrent profile
  writers;
- iframe, popup, tab, or worker browsing-context transport;
- persistent event delivery while an engine has no active document realm;
- full `StorageEvent`/`Storage` Web IDL identity, quota policy, or IndexedDB;
- browser task-source fairness beyond draining at the next host operation.

The parent may receive a worker's session-storage mutation for local worker
state synchronization, but only local-storage events enter the cross-document
coordinator. Profile persistence remains the existing bounded JSON mechanism;
writer locking is intentionally a later production gate.

## Paths

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_delivers_local_storage_events_between_documents -- --nocapture` — 1 passed, 360 filtered
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 361 passed, 0 failed

Remote CI remains pending this local checkpoint. Source push, release, tag,
registry publication, and issue-closure claims remain out of scope for this
slice.
