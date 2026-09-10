---
id: native-engine-browser-079
scope: glass-browser/native-engine/indexeddb-journal-convergence
status: done
depends-on: [native-engine-browser-078]
---

# BE-08: cross-process IndexedDB journal convergence

## Objective

Make the bounded IndexedDB profile state converge across independently live
native engines and sandboxed content workers. A writer must publish only its
validated origin changes, a receiver must merge those changes without
discarding unrelated databases or records, and a profile restart must retain
the same durable state. The existing two-crate boundary and bounded journal
ownership remain unchanged.

## Contract

- IndexedDB mutations are represented as bounded typed deltas: database
  replacement, object-store replacement, store-metadata update, record put,
  and record deletion. Each delta carries the validated storage key and is
  capped by `MAX_NATIVE_INDEXED_DB_CHANGES` (128) per operation.
- Local realms and content workers compute deltas from their committed
  origin state after each script, mutation, load, or close boundary. The
  parent applies the deltas to its full bounded state, persists them by
  re-reading the latest profile under `P.lock`, and appends them as one
  profile-adjacent journal record. Web Storage events retain their existing
  record format and routing.
- A live receiver polls the same journal before its next page operation,
  ignores its own writer records, applies validated IndexedDB deltas in
  journal order, and sends the current origin state into its persistent local
  realm or the current content-worker realm. Storage-event delivery remains
  independent, so an IndexedDB-only record is not dropped merely because it
  has no Web Storage event.
- Content workers receive a bounded full IndexedDB state only through the
  existing typed `storage_state` IPC synchronization command. Worker
  responses carry validated deltas rather than repeating the full snapshot;
  the worker never opens the profile, journal, lock, or reader-lease files.
- Reader-lease recovery remains authoritative: a missing lease or out-of-range
  cursor reloads the revisioned full profile and replaces the current realm
  state. Recovery does not replay discarded IndexedDB callbacks. A same-name
  or same-record conflict follows journal order, so the later validated delta
  is the explicit last-writer result; independent databases, stores, and
  records merge key-by-key where their schemas permit it.

## Ownership and sequence

```text
realm mutation -> before/after origin diff -> parent validates and applies
       |                                             |
       +-> profile snapshot merge under P.lock      +-> append P.events delta
                                                        |
live page operation -> read P.events -> apply deltas in order
       |                    |
       |                    +-- missing lease/cursor -> reload P snapshot
       +-> current-origin state replacement -> next realm operation
```

The durable profile is the authority after each locked write. The journal is
the bounded live-delivery path and carries both Web Storage events and
IndexedDB-only changes. A delta batch is intentionally not a general
multi-object transaction log: if a writer changes a database schema while an
independent writer changes the same database concurrently, journal order is
the conflict rule and the bounded schema/transaction limitations remain
visible.

## Deliberate boundary and tradeoffs

Delta records avoid repeatedly transferring the full 64 KiB IndexedDB state
over every worker response and prevent disjoint stale writers from erasing
unrelated durable state. They add diffing, validation, journal bytes, and a
bounded state-replacement message when a receiver's persistent realm must be
refreshed. A database version change or creation/deletion can still require a
bounded full database replacement, and same-database schema conflicts retain
last-writer behavior rather than pretending to provide browser-grade
version-change blocking.

The runtime remains a JSON-only IndexedDB subset. Indexes, cursors, key ranges,
binary/blob/date/regexp/map/set structured-clone values, quota-management
APIs, live connection/version-change coordination, complete transaction
scheduling, and full Web IDL identity remain open. The slice proves durable
and live convergence for the supported model; it does not claim general
browser parity.

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

Evidence is recorded after the affected compile, focused behavior, full
native-engine, documentation, and release-truth gates complete. The checkout
is local-only: no push, remote CI, release, tag, or registry-publication claim
is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine indexed_db -- --nocapture` — 4 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 373 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 729 Markdown files; 345 full-product MCP tools (100 browser-only); 17 examples; 22 public modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides routed/audited; 19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` — 729 Markdown documents; 83 current documents; 59 previous-version hits; 902 semantic audit hits; 0 current-claim failures
