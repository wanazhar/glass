---
id: issue64-task-scheduler-recovery-001
scope: issue-64-durable-task-scheduler-checkpoints
status: complete
depends-on: [issue64-task-thread-journey-001]
---

# Durable TaskScheduler checkpoints and restart recovery

## Objective

Preserve project-local task identity, assignment, lifecycle state, retry
deadline, and evidence across workspace actor restarts. Do not replay an
assigned prompt when its prior engine outcome is unknown.

## Behavior

- The versioned checkpoint is `.glass/tasks/scheduler.json`, bound to the
  canonical project root, bounded to 2 MiB and 128 task records. Writes use a
  same-directory temporary file, file sync,
  atomic replacement, and directory sync. Unix directory/file modes are 0700
  and 0600.
- A reopened assigned task in `ready`, `running`, `waiting`, or `verifying`
  becomes `paused` with `recoveryPending: true`; the prior agent ID, task
  prompt, and evidence remain available for inspection. The marker itself
  means the prior outcome is unknown; explanatory `lastError` text is added
  when checkpoint capacity permits.
- Recovery-pending assignments are excluded from agent event matching and the
  automatic scheduler. An explicit resume clears the old assignment and
  schedules a new attempt. This is an operator-directed retry, not proof of
  the prior attempt's outcome.
- Explicit cancel and reassign tolerate a missing historical worker after
  restart, while cancelling it when that worker is still live. Reopened task
  fields are revalidated against current worktree and verification policy.
- Scheduler mutations preflight the aggregate checkpoint bound before task,
  evidence, or worker dispatch changes. Oversized runtime event detail is
  reduced to a bounded scalar summary preserving verifier fields. Under
  aggregate pressure, older duplicate evidence is evicted globally in stable
  timestamp/task order, preserving the newest evidence per kind per task.
  The same fitting policy covers recovery, verifier, and agent-failure state
  transitions. Incoming event evidence is compressed and omitted only if
  needed to retain the task transition. `evidenceTruncated` marks tasks whose
  bounded evidence has been pruned or compressed. Each task retains at most
  128 evidence entries; this is bounded history rather than an unlimited audit
  log. Persisted failure detail is sanitized and capped at 160 bytes; if the
  checkpoint still cannot fit the detail, the failure state is retained and
  the detail omitted. The event cursor belongs to the live agent registry; after restart,
  assigned historical workers are recovery-pending and their old events are
  ignored, so checkpoint saturation cannot pin refresh on a replayed event.
- Queued tasks without an assignment remain queued and can be scheduled after
  dependencies are checked. Terminal task state and evidence are preserved.
- Missing checkpoints initialize an empty scheduler. Invalid, unsupported, or
  over-bound checkpoints fail initialization rather than silently discarding
  task history.

## Acceptance

- [x] Stable IDs, assignment, state, evidence, and next task ID survive
      deterministic scheduler reconstruction.
- [x] An assigned in-flight task reopens recovery-pending and remains
      undispatched through refresh.
- [x] Explicit resume clears the marker, creates a new assignment, and
      dispatches the prompt once in the reconstruction test.
- [x] Unknown checkpoint versions and oversized checkpoint files are rejected.
- [x] A checkpoint copied from another project and an ID counter that could
      reuse a restored task ID are rejected.
- [x] Restored worktree and verifier policy are revalidated; explicit recovery
      controls handle absent historical workers.
- [x] Capacity rejection leaves task/evidence/dispatch state unchanged;
      cross-task saturation prunes duplicate evidence while preserving each
      task's newest entry per kind and marks `evidenceTruncated`; the retained
      evidence does not falsely satisfy a verifier, refresh advances, and
      restart ignores events for recovery-pending assignments.
- [x] A near-cap agent failure with a 64 KiB error persists as `failed` with
      bounded readable error detail and survives reconstruction.
- [x] The current [Task DAG guide](../../task-dag.md) and delivery plan describe
      checkpoint bounds, permissions, and recovery limits.

## Verification

`cargo test -p glass-dev --lib tasks::tests --locked` passed: 20 tests, 0
failures. The tests use the deterministic fake agent backend; no provider or
model request is made. A full allocated-TUI restart journey and recovery of
the actual engine session remain outside this slice.
