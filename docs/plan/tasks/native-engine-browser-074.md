---
id: native-engine-browser-074
scope: glass-browser/native-engine/revisioned-web-storage-profile-merge
status: done
depends-on: [native-engine-browser-073]
---

# BE-02/BE-03/BE-04/BE-07: revisioned Web Storage profile convergence

## Objective

Prevent independent native engine owners from overwriting unrelated
`localStorage` mutations when their in-memory profile snapshots are stale.
Keep the existing bounded complete-snapshot file and retained OS lock, but
re-read the latest profile under the exclusive lock and apply the writer's
recorded local-storage mutation deltas before committing a new revision.

## Contract

- The existing version-1 profile format gains an optional `revision` field.
  Profiles written before this field existed deserialize as revision zero and
  are upgraded on the next successful write.
- A profile write holds the existing exclusive lock while it reads and
  validates the latest profile, applies the caller's local-storage mutation
  descriptors in order, increments the profile revision, encodes the bounded
  merged snapshot, and completes the existing temporary-file commit.
- Only effective local-storage mutations are merged. Session-storage changes
  remain volatile and are never serialized; incoming cross-document events
  without a local mutation delta do not overwrite a newer profile.
- Set/remove/clear deltas preserve lock-order last-writer behavior for the same
  origin/key while retaining unrelated keys from another stale writer. A
  clear delta intentionally applies to the latest origin map at commit time.
- A malformed or over-limit latest profile remains a typed failure. The writer
  does not silently fall back to its stale snapshot or to Chromium/CDP.
- Revisioning and delta merge converge profile file state only. They do not
  provide cross-process `storage` event delivery, cookie profile persistence,
  IndexedDB, quota policy, or full browser-profile parity.

## Ownership and sequence

```text
runtime mutation descriptors
  -> exclusive P.lock
  -> read and validate latest revisioned profile
  -> apply local deltas to latest local map
  -> increment revision
  -> bounded temporary snapshot and commit
  -> release handle
```

The runtime owner records effective changes before persistence. The profile
writer is the merge authority; the in-process event coordinator remains the
live local event authority and is intentionally separate from file convergence.

## Deliberate boundary and tradeoffs

This is a bounded per-mutation merge, not a general transaction log. It keeps
the existing full-snapshot recovery and size limits while preventing the
specific stale-snapshot data loss found after 072. Lock acquisition order is
the conflict order for the same key; there is no wall-clock or causal merge
across processes. A clear operation wins over keys present at its commit-time
origin map, as Web Storage users expect.

The profile revision is a commit generation, not a public browser revision and
is not exposed as page state. Cross-process event IPC, profile ownership,
cookie persistence, IndexedDB, and the wider browser-complete contract remain
open for later slices.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Final verification is recorded after the affected native target and docs
gates complete:

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine stale_profile -- --nocapture` — 2 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine storage -- --nocapture` — 11 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 367 passed, 0 failed
- `python3 scripts/check-documentation-coverage.py` — 724 Markdown files; 345 full-product MCP tools (100 browser-only); 17 examples; 22 public modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides routed/audited; 19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` — 724 Markdown documents; 83 current documents; 59 previous-version hits; 898 semantic audit hits; 0 current-claim failures

Strict affected-package Clippy remains a known baseline gate with 32
pre-existing diagnostics; it is not relabeled as passing by this slice.
