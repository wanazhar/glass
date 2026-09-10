---
id: native-engine-browser-085
scope: glass-browser/native-engine/indexeddb-transaction-queue
status: done
depends-on: [native-engine-browser-084]
---

# BE-08: bounded IndexedDB transaction serialization

## Objective

Prevent same-realm IndexedDB transactions from interleaving in a way that
could make rollback erase a neighboring transaction’s work. Transactions for
one database must run in order, and a queued transaction must capture its
rollback state only after preceding work has completed.

## Contract

- The realm maintains a bounded per-database transaction queue. At most one
  transaction for a database executes a request operation at a time.
- Requests within a transaction run in insertion order. Request callbacks run
  before the next queued operation is started, preserving the existing async
  request lifecycle and allowing callbacks to append more requests.
- A transaction captures its JSON rollback snapshot when it begins execution,
  not when it is merely created. A queued transaction therefore observes the
  committed state of the transaction before it and cannot restore an older
  snapshot over that work.
- Completion and abort release the queue before their terminal callback;
  queued transactions then make progress. Aborted transactions still deliver
  one `onabort` and never `oncomplete`.
- The queue is same-realm and name-keyed, uses the existing bounded request,
  state, and resource limits, and does not retain profile file handles.
  Parent-side profile locking/journal merge remains the authority for
  independent Glass engines and content workers.

## Ownership and sequence

```text
tx A: write -> abort/rollback -> onabort -> release
tx B:                         queued -> begin from committed state -> commit
```

The JavaScript realm owns ordering and transaction snapshots. Rust observes
only the final realm state and continues to validate, merge, journal, and
persist it through the existing parent boundary.

## Deliberate boundary and tradeoffs

Serializing one realm makes rollback deterministic and avoids adding a new
profile protocol, at the cost of less parallelism. It does not coordinate
transactions across independent JavaScript realms or Glass processes, model
all IndexedDB lock modes, implement request `preventDefault()` recovery, or
restore the pre-upgrade version when an upgrade callback fails. Full
structured-clone types, quota permission policy, and the remaining
browser-complete gates remain future work.

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
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_indexed_db_serializes_transactions_before_abort -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_indexed_db_ -- --nocapture` — 8 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 380 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 735 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 735 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=908; current-claim failures=0
