---
id: native-engine-browser-072
scope: glass-browser/native-engine/web-storage-profile-io-locking
status: done
depends-on: [native-engine-browser-071]
---

# BE-02/BE-03/BE-04/BE-07: bounded Web Storage profile I/O locking

## Objective

Coordinate the existing opt-in JSON Web Storage profile across native content
workers and independent Glass processes. Profile reads use a shared operating
system advisory lock; profile snapshot writes use an exclusive lock that spans
the complete bounded write and commit path. A contended lock fails with a typed
native error after a short bounded retry window instead of falling back to
Chromium/CDP or reading a potentially changing snapshot.

## Contract

- The profile path `P` uses the retained lock path `P` with its extension
  replaced by `.lock`. The lock file is created as needed and is not itself
  treated as profile data.
- `load_web_storage_profile` holds a shared lock while checking the bounded
  metadata length, reading the bytes, decoding the versioned profile, and
  validating the resulting state.
- `save_web_storage_profile` holds an exclusive lock while writing the bounded
  temporary snapshot and completing the platform-specific rename/copy commit.
  The existing complete-snapshot-before-commit behavior remains in force.
- Lock acquisition retries for at most 500ms. A still-contended profile
  returns `NativeEngineError::StorageProfileLocked` with the profile path.
  Other lock/open failures remain typed worker failures. There is no implicit
  backend fallback.
- The Linux sandbox exposes the retained profile lock file alongside the
  profile data file, so a sandboxed worker can participate in the same OS lock
  protocol. The worker continues to use the existing bounded profile path.
- Lock ownership is held by the file handle and is released by the operating
  system on normal drop or process exit; a stale lock pathname cannot by itself
  block recovery after a crashed worker.

## Ownership and sequence

```text
profile read/write request
  -> open retained P.lock
  -> bounded shared or exclusive OS lock
  -> bounded JSON read or complete temporary snapshot
  -> atomic rename / verified platform fallback
  -> release handle
```

The lock coordinates physical profile I/O, not page-event delivery. The
in-process storage coordinator from 070/071 remains the source of live event
fan-out between engines in one Glass process.

## Deliberate boundary and tradeoffs

This slice prevents concurrent readers from observing a Windows copy fallback
mid-write and serializes independent profile file operations. It does not merge
two stale full-state snapshots: two independent processes that both modify the
same profile can still have last-snapshot-wins logical behavior after their
individual writes are serialized. A later profile ownership or revisioned
delta protocol must either reject that usage or merge it explicitly before the
native backend can claim multi-process profile parity.

This slice also does not add `sessionStorage` browsing-context identity or
cross-process storage-event IPC, iframe/popup/tab topology, IndexedDB, quota
policy, full Storage Web IDL identity, or browser parity. Those remain explicit
issue #40 boundaries.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/error.rs`
- `crates/glass-browser/src/browser/native_engine/sandbox.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_web_storage_profile_rejects_an_active_writer -- --nocapture` — 1 passed, 361 filtered
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine web_storage -- --nocapture` — 6 passed, 356 filtered
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 362 passed, 0 failed
- `python3 scripts/check-documentation-coverage.py` — 722 Markdown files; 345 full-product MCP tools; 17 examples; 22 public modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides routed/audited; 19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` — 722 Markdown documents; 83 current documents; 59 previous-version hits; 898 semantic audit hits; 0 current-claim failures

The strict affected-package command
`cargo clippy --quiet -p glass-browser --features native-engine --all-targets -- -D warnings`
was also attempted. It remains red on 32 pre-existing diagnostics in
untouched native backend, content-process, DOM, engine, resource-loader,
sandbox, and runtime-worker code; none points at the 072 additions. Those
unrelated baseline lints are not silently represented as a passing 072 gate.

Issue-record verification is the remaining closeout check for this checkpoint.
Remote CI remains pending this local checkpoint. Source push, release, tag,
registry publication, and issue-closure claims remain out of scope for this
slice.
