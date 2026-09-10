---
id: native-engine-browser-077
scope: glass-browser/native-engine/storage-journal-retention-recovery
status: done
depends-on: [native-engine-browser-076]
---

# BE-02/BE-03/BE-04/BE-07: bounded storage-journal retention and recovery

## Objective

Make the profile-adjacent Web Storage event journal bounded in operation as
well as in bytes. Live readers must retain their unconsumed records while
acknowledged prefixes can be reclaimed, and a crashed, expired, missing, or
otherwise stale reader must recover from the authoritative profile snapshot
without replaying callbacks that may already have been delivered.

## Contract

- A profile at `P` keeps its newline-delimited journal at `P.events` and a
  bounded reader-lease file at `P.readers`. Both are coordinated with the
  retained advisory lock at `P.lock`; the content worker never opens either
  journal or lease path.
- A profile-backed engine registers its unique writer identity as a reader
  lease. The lease stores a bounded byte cursor and wall-clock heartbeat. At
  most 128 leases are retained; a 15-minute-old lease is stale and a healthy
  reader refreshes its lease at most once per 30 seconds or when its cursor
  advances. Explicit close removes the lease. A dropped process is reclaimed
  by the stale-lease rule.
- Journal reads and appends hold the exclusive profile lock for the complete
  bounded operation. A reader cursor is advanced only through complete
  newline records. A partial tail remains undelivered and is repaired by the
  next append; initialization and reader reads use the complete-record length
  rather than raw file metadata.
- When an append would exceed the 4 MiB journal cap, compaction may drop only
  a complete prefix acknowledged by every live reader lease. Every retained
  cursor is shifted by the dropped byte count before the append becomes
  visible. If a live reader still pins too much unconsumed data, the append
  returns the existing typed limit error; it does not silently discard that
  reader's records.
- A missing reader lease or a cursor beyond the complete journal length is a
  recovery signal, not a replay instruction. The engine advances to the
  current complete end, reloads the bounded revisioned profile snapshot, and
  replaces the local or content-worker storage state through a typed full-state
  IPC command. Storage-event callbacks dropped by this recovery are not
  replayed. The snapshot is authoritative for durable `localStorage`; the
  active engine's volatile `sessionStorage` remains in memory.
- The parent engine remains the sole journal writer for local and sandboxed
  documents. Content workers continue to receive only bounded full-state or
  event commands over IPC, preserving the two-installable-crate boundary and
  preventing duplicate journal records. The new full-state command is part of
  content-worker protocol version 3.

## Ownership and sequence

```text
engine start -> cursor at complete journal end -> register P.readers lease
      |
page operation -> exclusive read + lease heartbeat/cursor update
      |                         |
      |                         +-- missing/stale cursor -> reload P snapshot
      |
storage mutation -> merge P snapshot -> append journal
      |                         |
      |                         +-- cap -> compact only all-reader-acknowledged prefix
      |
content worker <- full storage snapshot or filtered StorageEvent records
engine close -> persist owned state -> unregister P.readers lease
```

The profile snapshot is the recovery authority, while the journal is the live
delivery and retention mechanism. This intentionally does not claim a general
transaction log: a crash between snapshot and journal commits may lose an
event callback, but state recovery remains deterministic and bounded.

## Deliberate boundary and tradeoffs

Lease files add one small bounded metadata write at registration, cursor
advancement, heartbeat, compaction, and close. The exclusive read path is
short and bounded by the 4 MiB journal and 64 KiB lease-file limits, but it is
more serialized than the former shared-read path. The 15-minute stale window
protects an interrupted reader's unconsumed records for a useful recovery
period; afterward, retaining those records would make disk usage and forward
progress hostage to a dead process, so the profile snapshot wins and event
callbacks are intentionally not replayed.

Compaction can reclaim all old records when no live lease remains, or only the
prefix acknowledged by every live lease when readers are active. A slow but
healthy reader therefore can still make the journal hit its typed 4 MiB limit;
that is visible backpressure rather than silent data loss. Session storage is
not durable in the profile and is preserved only by a still-live engine during
snapshot recovery. IndexedDB, full cookie policy/Web IDL parity, and the other
browser-complete gates remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected native target, focused recovery,
full-native, documentation, and release-truth gates complete. Remote CI, push,
release, tag, and registry-publication claims are intentionally absent while
this branch remains local-only.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine storage_event_journal_ -- --nocapture` — 2 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_recovers_after_storage_reader_lease_loss -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 369 passed, 0 failed
- `python3 scripts/check-documentation-coverage.py` — 727 Markdown files; 345 full-product MCP tools (100 browser-only); 17 examples; 22 public modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides routed/audited; 19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` — 727 Markdown documents; 83 current documents; 59 previous-version hits; 899 semantic audit hits; 0 current-claim failures

Strict affected-package Clippy returned the expected non-zero baseline:
`cargo clippy --quiet -p glass-browser --features native-engine --all-targets -- -D warnings`
reported exactly 32 documented pre-existing diagnostics and no diagnostics
introduced by this slice. It is not relabeled as a pass.
