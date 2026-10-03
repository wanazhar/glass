# Verification review: native-engine-browser-842

Checkpoint reviewed: `f18be2e7fa4f26d39ba9a0c3d659f907907c5c68`
Scope: committed checkpoint and requested process-backed regression. No
production-code edit or commit was made for this review.

## Findings

### P2 — blocking: required process-backed regression timed out

- Design/verification: `docs/plan/tasks/native-engine-browser-842.md`,
  Verification, requires
  `native_runtime_shared_worker_cookie_changes_survive_stale_create_snapshots`.
- Test: `crates/glass-browser/tests/native_engine.rs`, around line 34654.
- Exact command:
  `timeout --signal=INT --kill-after=5s 90s cargo test -p glass-browser --features native-engine --test native_engine --locked native_runtime_shared_worker_cookie_changes_survive_stale_create_snapshots -- --exact`
- The timeout wrapper exited 124 after 1m30.037s. The test binary started and
  reported `running 1 test`, but emitted no test result before timeout. A
  post-timeout process check found no matching Cargo test command or
  `native_engine` test process. The cookie snapshot, override replay, and
  deletion behavior therefore remain unverified by this gate.

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

## Conclusion

**blocked** — static review found the lock snapshot and ordering semantics
sound, but the required process-backed regression timed out without a result.
