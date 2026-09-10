---
id: native-engine-browser-080
scope: glass-browser/native-engine/indexes-cursors-key-ranges
status: done
depends-on: [native-engine-browser-079]
---

# BE-08: bounded IndexedDB indexes, key ranges, and cursors

## Objective

Expand the bounded JSON IndexedDB surface with the query primitives used by
ordinary data-backed pages: object-store indexes, key-range predicates, and
ordered cursors. The metadata must survive profile restart and participate in
the existing key-level delta protocol without widening the two-crate
boundary or claiming complete IndexedDB/Web IDL parity.

## Contract

- Each object store may retain at most 128 named indexes. Index metadata is
  persisted beside the existing key path, auto-increment, and record state;
  profiles written before the `indexes` field remain readable. Indexes use a
  bounded string key path, `unique`, and `multiEntry`; compound key paths are
  explicitly rejected.
- `IDBObjectStore.createIndex`, `deleteIndex`, `index`, and sorted
  `indexNames` are available only in the existing version-change transaction
  boundary. Unique constraints are checked while creating an index and on
  every subsequent `put`, `add`, or cursor update. Multi-entry arrays produce
  distinct index entries for the supported string/finite-number key subset.
- `IDBKeyRange.only`, `lowerBound`, `upperBound`, and `bound` produce bounded
  range objects with ordered numeric-before-string comparison, open/closed
  bounds, and `includes`. Store and index `get`, `getKey`, `getAll`,
  `getAllKeys`, `count`, and store `delete` accept exact keys or ranges.
- Store and index `openCursor` and `openKeyCursor` support `next`,
  `nextunique`, `prev`, and `prevunique`. Cursor results expose source,
  direction, key, primary key, and JSON-cloned value; bounded `continue`,
  `continuePrimaryKey`, `advance`, `update`, and `delete` are ordered through
  the existing transaction request queue.
- The parent validates index metadata through the existing profile and journal
  boundary. Index-schema changes use a dedicated index-set delta so a stale
  writer does not replace unrelated records; record deltas remain mergeable
  under the established journal-order conflict rule. Content workers return
  deltas and never open profile or journal files.

## Ownership and sequence

```text
upgrade transaction -> create/delete index metadata -> validate uniqueness
        |
query -> normalize key/range -> bounded ordered index scan -> request callback
        |
cursor callback -> continue/advance/update/delete -> queued next result
        |
profile boundary -> index-set/record deltas -> locked merge -> journal delivery
```

The implementation derives index entries from the bounded record map at query
time. This keeps the durable format small and makes stale-profile merges
straightforward; the explicit tradeoff is bounded scan work rather than a
separate on-disk index structure.

## Deliberate boundary and tradeoffs

The key model remains strings and finite numbers, with JSON values and string
key paths. Compound keys, binary/blob/date/regexp/map/set values, array-valued
keys outside `multiEntry`, and complete IndexedDB key ordering are outside
this profile. A unique index is fail-closed on constraint conflict. Cursor
updates that would change the primary key are rejected, and same-database
schema/version conflicts continue to follow journal order instead of
pretending to implement browser-grade connection blocking/version-change
coordination.

Each query performs at most the existing 128-record store scan and each
cursor is capped by the same bounded result/request limits. This is adequate
for the native profile’s bounded state and predictable for resource use, but
it is not a throughput claim for large browser databases; a future profile
revision may need materialized-index budgets if larger stores are admitted.

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
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine indexed_db_ -- --nocapture` — 5 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 374 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 730 Markdown files; 345 full-product MCP tools (100 browser-only); 17 examples; 22 public modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides routed/audited; 19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` — 730 Markdown documents; 83 current documents; 59 previous-version hits; 903 semantic audit hits; 0 current-claim failures
