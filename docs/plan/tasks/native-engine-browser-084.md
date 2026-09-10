---
id: native-engine-browser-084
scope: glass-browser/native-engine/storage-manager-quota
status: done
depends-on: [native-engine-browser-083]
---

# BE-08: bounded StorageManager quota surface

## Objective

Expose the basic quota-facing Web API needed by ordinary native pages without
claiming a browser-wide quota or permission system. The native realm should
report a stable bounded quota, a usage estimate that reflects its current
bounded storage maps, and an explicit answer for persistence permission.

## Contract

- `navigator.storage.estimate()` returns a Promise with numeric `usage` and
  `quota` fields. Usage is a deterministic JSON-size estimate of the current
  local/session/IndexedDB maps, capped at the native profile quota.
- The native quota is the existing 4 MiB bounded storage-profile ceiling;
  IndexedDB’s smaller per-origin/database/store/value limits remain enforced
  separately by their request paths.
- `navigator.storage.persist()` and `navigator.storage.persisted()` are
  asynchronous, stable APIs that return `false`. No persistence permission
  prompt or silent durability escalation is implied.
- The StorageManager object is installed in the existing native navigator
  object and remains available across repeated page bootstrap evaluations.
- Estimation is observational only: it does not reserve space or bypass the
  existing Rust profile validation, lock, merge, journal, or IPC boundaries.

## Ownership and sequence

```text
page -> navigator.storage.estimate()
          |-> bounded JSON usage + fixed profile quota
page -> persist()/persisted() -> false (permission policy not implemented)
```

The JavaScript realm owns the Web API projection. Rust remains authoritative
for profile-size validation, persistence, cross-process merge, and recovery.

## Deliberate boundary and tradeoffs

JSON-length accounting is cheap, deterministic, and safe for the bounded
implementation, but it is not browser-native disk accounting and does not
include every engine cache. The implementation does not grant persistence,
implement quota prompts, reserve quota transactionally, arbitrate quota across
processes, or provide the rest of StorageManager (directory access, buckets,
durability modes). Structured-clone values, complete transaction scheduling,
cross-process connection identity, and the remaining browser-complete gates
remain future work.

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
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_storage_manager_reports_bounded_estimate -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 379 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 734 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 734 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=907; current-claim failures=0
