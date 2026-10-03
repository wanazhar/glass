# Verification review: native-engine-browser-842

Checkpoint reviewed: `f18be2e7fa4f26d39ba9a0c3d659f907907c5c68`
Scope: committed checkpoint and requested process-backed regression. No
production-code edit or commit was made for this review.

## Findings

### Initial attempt — timeout, resolved by follow-up

- Design/verification: `docs/plan/tasks/native-engine-browser-842.md`,
  Verification, requires
  `native_runtime_shared_worker_cookie_changes_survive_stale_create_snapshots`.
- Test: `crates/glass-browser/tests/native_engine.rs`, around line 34654.
- Exact command:
  `timeout --signal=INT --kill-after=5s 90s cargo test -p glass-browser --features native-engine --test native_engine --locked native_runtime_shared_worker_cookie_changes_survive_stale_create_snapshots -- --exact`
- The timeout wrapper exited 124 after 1m30.037s. The test binary started and
  reported `running 1 test`, but emitted no test result before timeout. The
  command's bound included integration-target compilation, leaving too little
  runtime to determine the test outcome. The no-build follow-up below resolves
  this initial timeout and verifies the cookie snapshot, override replay, and
  deletion behavior.

### Static review — no code finding

- Design: `docs/architecture/native-engine.md#cookie-authority-and-process-boundary`
  and `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`.
- Code: `NativeEngineBackend::deliver_cookie_changes` in
  `crates/glass-browser/src/browser/native_backend.rs` (around lines
  6067–6148); active-target selection (around lines 3641–3685); focused
  lock-liveness test (around line 10565).
- The method snapshots the selected active engine and parked owner handles
  while holding the target registry, then releases that guard before awaiting
  any owner. Target selection updates the active engine and target ID under
  the same registry lock, making the active-owner snapshot coherent. Recipient
  order remains active engine, active parked frames, then parked targets and
  their parked frames. The first applicable owner persists the change; later
  owners receive runtime-only updates, with errors propagated as before.
- The socket-free test holds the active owner, observes delivery waiting, and
  checks that the target registry remains available. It is a focused lock
  regression; it does not replace the timed-out process-backed behavior test.

## Follow-up

The initial 90-second Cargo timeout while rebuilding the test target remains part of the historical record. A separate test agent ran the already-built binary for the same committed code, with no intervening rerun or code change:

```text
timeout --signal=INT --kill-after=5s 90s target/debug/deps/native_engine-d4489efcb3153c24 native_runtime_shared_worker_cookie_changes_survive_stale_create_snapshots --exact --nocapture
```

Result: exit 0; 1 passed, 0 failed; elapsed 48.37s (test runtime 48.31s). The P2 is resolved by this successful run. The static review of the `f18be2e7` lock snapshot remains unchanged.

## Conclusion

**pass** — static review found no code defect in the owner snapshot, ordering,
or persistence semantics, and the required process-backed regression passed.
