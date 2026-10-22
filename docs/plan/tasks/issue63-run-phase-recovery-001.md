---
id: issue63-run-phase-recovery-001
scope: issue-63-durable-run-phase-recovery
status: complete
depends-on: [issue63-core-session-resume-001]
---

# Durable worker phase recovery

## Objective

Persist the current phase of each resident worker incarnation so a restart can
explain whether it was idle, working, or waiting before Glass stopped. On
reopen, initialized Core sessions remain explicitly recovery-pending; persisted
phase is context only and never claims that a process is live or replays work.

## Acceptance

- [x] The bounded run classification records queued, starting, idle, working,
      waiting, recovery-pending, and terminal phases.
- [x] Ready, request-start, waiting, settled, and idle-response transitions
      update the durable phase.
- [x] A reopened Core session with a prior active or idle phase is shown as
      recovery-pending with the last phase in its explanation. Runs without an
      initialized Core session remain unavailable.
- [x] Waiting events update both the durable classification and live status.
- [x] Focused tests verify phase persistence across transitions and truthful
      reopen behavior for idle, working, and waiting runs.
- [x] Focused `glass-dev` tests, formatting, and diff review pass.

## Limits

`RunId` identifies a worker incarnation, not an individual submitted turn.
Per-send dispatch and exact-response tracking is being added by the
[send-attempt recovery task](issue64-send-attempt-recovery-001.md).
A run-level settled engine event is not verification or review approval.
