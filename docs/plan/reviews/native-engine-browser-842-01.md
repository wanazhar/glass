# Verification review: native-engine-browser-842

Checkpoint reviewed: `cf149bef3bb3d583fa64a74c26e499638d6d4f85`  
Scope: committed checkpoint only. No Cargo command or production-code edit was
made for this review.

## Findings

### P2 — blocking: document generation can become stale after the owner check

- Design: `docs/plan/tasks/native-engine-browser-842.md`, Contract, especially
  the requirement that a stale or changed owner fail explicitly; see also
  `docs/architecture/native-engine.md#cookie-authority-and-process-boundary`.
- Code: `crates/glass-browser/src/browser/native_backend.rs`,
  `NativeEngineBackend::create_shared_worker_connection` (around lines
  5559–5597); in-place navigation is handled by `navigate_active_frame_request`
  (around lines 6119–6191).
- The captured document generation is checked while the exact owner mutex is
  held, and the parent cookie profile is cloned there. After that guard is
  released, the code resolves the frame again but checks only
  `Arc::ptr_eq`. Navigation mutates the document on the existing engine owner,
  so the same `Arc` can remain current after its document generation changes.
  A create that becomes stale in this interval can therefore pass the identity
  check, insert its transfer route, and seed the coordinator from the earlier
  cookie snapshot while retaining the request's old generation. Owner identity
  revalidation does not close the document-generation race required by the
  task contract.
- Keep this blocking until generation validation and route registration are
  synchronized against document replacement, so a generation change in this
  interval is rejected rather than accepted with the old snapshot.

### P2 — blocking: required process-backed acceptance test did not execute

- Design/verification: `docs/plan/tasks/native-engine-browser-842.md`,
  Verification, which requires the parent/backend regression
  `native_runtime_shared_worker_cookie_changes_survive_stale_create_snapshots`.
- Test/evidence: `crates/glass-browser/tests/native_engine.rs`, that test
  (around line 34552), and the task's Current Evidence section.
- The task accurately records that the exact test attempt timed out while
  linking the integration target and that the test executable never started.
  Thus the checkpoint has no process-backed pass evidence for the parent
  cookie snapshot, override replay, and deletion behavior. The earlier busy
  failure and the passing content-process route test are not substitutes for
  this parent/backend gate. Do not mark the slice verified until this focused
  test starts and passes.

### P3 — non-blocking: early errors can leave a bounded orphan route

- Code: `insert_page_message_port_routes` (around lines 4834–4875) and
  `create_shared_worker_connection` (around lines 5593–5612) in
  `crates/glass-browser/src/browser/native_backend.rs`.
- The special insertion path enforces `NATIVE_MAX_MESSAGE_PORT_ROUTES`, and
  later worker-creation failures plus normal close/document/context teardown
  remove routes. However, it inserts the route before acquiring the
  coordinator and allocating the connection ID. Errors from the coordinator
  lock or the connection-ID conversion/increment return via `?` before the
  cleanup paths, leaving that bridge key in the route map. This cannot grow
  without bound because insertion is capped, but can consume capacity; the
  special path also skips the stale-route pruning performed by generic
  registration.

## Conclusion

**blocked** — the post-snapshot generation race violates the stale-owner
contract, and the required process-backed regression did not execute. The
status documentation is honest: the task and plan keep Slice 842 in progress,
record the timeout before test startup, and distinguish the remaining general
content-process mirror/broker work in Slice 843. No documentation claim says
the required parent regression passed.
