---
id: issue64-task-thread-queue-restart-001
scope: issue-64-combined-task-thread-queue-recovery
status: complete
depends-on: [issue64-task-scheduler-recovery-001, issue64-task-thread-journey-001, issue64-queue-retarget-retry-001]
---

# Combined task, thread, and queue recovery journey

## Objective

Verify the project-local TUI recovery path across TaskScheduler, Core-backed
agent threads, and the durable follow-up queue. Reopening the same project must
not replay an interrupted task prompt or dispatch a queued follow-up. Explicit
task recovery creates one new Core session/assignment, while the old queued
entry remains pinned to its original thread and run until the user explicitly
retargets it.

## Context

- [Task DAG and scheduler recovery](../../task-dag.md)
- [Durable scheduler checkpoint slice](issue64-task-scheduler-recovery-001.md)
- [Task-thread journey](issue64-task-thread-journey-001.md)
- [Queue retarget and retry journey](issue64-queue-retarget-retry-001.md)
- [Development TUI](../../architecture/development-tui.md)

This journey uses the deterministic fake Core runtime and does not establish
live provider behavior or recovery of an interrupted Core engine session.

## Path

- `crates/glass-dev/tests/tui_pty.rs`
- `crates/glass-dev/tests/fixtures/fake_glass_core_runtime.py` only if the
  existing fake runtime needs a deterministic gate or invocation log
- `docs/plan/README.md`
- `docs/plan/tasks/issue64-task-thread-queue-restart-001.md`
- `docs/plan/tasks/issue64-task-scheduler-recovery-001.md` only if its scope
  or evidence boundary needs correction

## Acceptance

- [x] Start one task in an allocated TUI and record its task, agent, thread,
  Core session, and run identities; save a queued follow-up pinned to that run.
- [x] Close Glass while the fake Core task turn is gated, then reopen the same
  project and data directory.
- [x] The task is `paused` and recovery-pending; the old thread/run and queue
  entry keep their identities; startup emits no old task or queue prompt.
- [x] The task-to-thread picker displays the correct task conversation after
  AgentRegistry reconstruction for the one-thread recovery case.
- [x] Explicit task resume creates a new assignment/session and sends the task
  prompt exactly once; it does not settle or retarget the old queue entry.
- [x] The old queue item remains pinned to its original thread and run through
  restart and task resume; no implicit retarget or queued prompt occurs.
- [x] The separate [queue retarget and retry journey](issue64-queue-retarget-retry-001.md)
  covers a genuinely stale RunId and explicit retarget. This restart journey
  does not manufacture a new current RunId for the old thread.
- [x] The test cleans only its own fixture processes, temporary project, and
  XDG data and passes through the existing opt-in allocated-PTY test gate.

## Verification

Validation passed: `GLASS_E2E=1 GLASS_E2E_BINARY=target/debug/glass cargo
test -p glass-dev --test tui_pty
task_thread_queue_restart_requires_explicit_core_recovery --locked --
--nocapture` (1 test). `cargo fmt --all -- --check` passed. The fixture held
the initial and resumed task turns at deterministic gates. Restart emitted
no task or queue prompt; the picker associated the recovered task with its
persisted thread; explicit task resume created a new assignment/session and
sent the task prompt exactly once. The queued row retained its original entry,
thread, and RunId, remained ready, and emitted no queue prompt. The fake runtime
does not establish real provider behavior or recovery of an interrupted Core
engine session. The single-thread fixture does not force a changed regenerated
AgentId; multi-agent ID reordering remains outside this journey unless a
deterministic setup is added. The full documentation validators and
`git diff --check` are run for the containing change before commit.
