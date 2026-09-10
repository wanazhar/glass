---
id: native-engine-browser-082
scope: glass-browser/native-engine/indexeddb-delete-lifecycle
status: done
depends-on: [native-engine-browser-081]
---

# BE-08: bounded IndexedDB deletion lifecycle

## Objective

Complete the same-realm IndexedDB deletion lifecycle started by 081. A
`deleteDatabase()` request must not remove a database behind a live connection:
connections receive a `versionchange` notification with `newVersion: null`, a
blocked request waits for them to close, and only then is the durable database
state removed.

## Contract

- `indexedDB.deleteDatabase(name)` validates the bounded database name and
  completes asynchronously through the existing request callback contract.
- Deleting an existing database dispatches one same-realm `versionchange`
  event to each live connection with its current `oldVersion` and
  `newVersion: null`. Handler exceptions do not prevent other connections from
  being notified.
- If any connection remains open, the request emits at most one `blocked`
  event and remains pending. Closing the final connection schedules deletion;
  the request then completes successfully with an undefined result.
- Deleting a missing database is a successful no-op. A subsequent unversioned
  `open()` observes a new database and can run its version-1 upgrade callback,
  proving that the previous state was removed.
- Pending deletes use a bounded name-keyed registry, do not retain profile
  file handles, and preserve the existing database/store/record/value limits.
  Profile persistence and cross-process delivery remain owned by the parent
  and existing journal/IPC contracts.

## Ownership and sequence

```text
delete(name) -> versionchange(A, newVersion=null)
                     |
             A remains open -> blocked -> A.close()
                     |                         |
                     +-------------------------+-> remove state -> success
```

The realm owns live connection identity and delete scheduling. The parent owns
the persisted origin snapshot and applies the resulting bounded state change
through the existing profile path.

## Deliberate boundary and tradeoffs

This slice prevents the most damaging same-realm deletion race without
pretending that a durable profile can identify live JavaScript objects in a
different process. Cross-process connection leases, full factory operation
queue ordering for every concurrent open/delete combination, complete
transaction rollback, and broader IndexedDB/Web IDL parity remain future
work. A cross-process delete protocol must be designed separately instead of
encoding connection identity into the profile journal.

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
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_indexed_db_delete_blocks_open_connection -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine indexed_db_ -- --nocapture` — 7 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 376 passed, 0 failed on the clean rerun; one earlier parallel timing run had the unrelated interval test at 375/376, and its isolated rerun passed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 732 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 732 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=905; current-claim failures=0
