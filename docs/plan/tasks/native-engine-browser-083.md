---
id: native-engine-browser-083
scope: glass-browser/native-engine/indexeddb-transaction-rollback
status: done
depends-on: [native-engine-browser-082]
---

# BE-08: bounded IndexedDB transaction rollback

## Objective

Make ordinary native IndexedDB write transactions atomic across request
failures and explicit aborts. A failed write must not leave earlier writes from
the same transaction visible, and an aborted transaction must report
`onabort` rather than falsely reporting `oncomplete`.

## Contract

- Each bounded transaction captures a JSON state snapshot at creation and
  restores the database state in place at most once when it aborts.
- A request operation error marks the transaction aborted, retains the typed
  request/transaction error, rolls back all writes from that transaction, and
  completes the request and transaction through the existing asynchronous
  callback order.
- `IDBTransaction.abort()` is idempotent while active, rolls back prior writes,
  and delivers one `onabort` callback after pending requests settle. Aborted
  transactions never deliver `oncomplete`.
- The rollback path preserves existing database/store/index/record/value
  limits and remains compatible with profile diffing: an aborted evaluation
  produces no durable IndexedDB delta for its reverted writes.
- The snapshot is bounded by the existing JSON IndexedDB state and does not
  retain profile handles. Cross-process and cross-transaction coordination
  still use the existing parent/journal boundary.

## Ownership and sequence

```text
write A -> write B fails -> request.onerror -> restore snapshot -> onabort
write A -> abort()       -> restore snapshot -----------------> onabort
```

The JavaScript realm owns transaction snapshots and callback lifecycle. The
Rust parent observes only the post-evaluation state and persists validated
changes through the existing profile and journal paths.

## Deliberate boundary and tradeoffs

Snapshot rollback makes a single transaction atomic without adding a new
profile format or a second persistence protocol. It does not yet serialize
independent concurrent transactions, implement request `preventDefault()`
error recovery, or restore the pre-upgrade version when an
`onupgradeneeded` callback fails. Full structured-clone types, quota APIs,
cross-process live connection identity, and the remaining browser-complete
gates remain future work.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, focused behavior, full
native-engine, documentation, and release-truth gates complete. The checkout
is local-only: no push, remote CI, release, tag, or registry-publication claim
is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_indexed_db_transaction_rolls_back_failed_write -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_indexed_db_explicit_abort_rolls_back_write -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_indexed_db_ -- --nocapture` — 7 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 378 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 733 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 733 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=906; current-claim failures=0
