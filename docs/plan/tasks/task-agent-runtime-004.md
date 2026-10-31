---
id: task-agent-runtime-004
scope: Core deferred child work and Glass coordination adapter
status: ready
depends-on: [task-agent-budget-002, task-agent-inbox-003]
---

# Runtime coordination integration

## Objective

Connect the reviewed budget/inbox foundations merged at `84f76e4b` through
real host dispatch, parking, verification and bounded owner continuation.
Keep automatic coordination opt-in until integrated acceptance passes.

## Context

Read [task 001](task-agent-coordination-001.md), the coordination design and
the independent reviews for tasks 002/003. Cost reservations and inbox storage
are foundations, not an enabled end-to-end feature.

## Path and owners

- Luna Secure: Core runtime/Agent/subagent deferred execution, parent-redacted
  events and narrow Glass runtime adapter. Negotiate capability explicitly;
  preserve the synchronous path for existing callers.
- Luna Capture: Glass scheduler cumulative limits, dispatch intent, task/todo
  binding reconciliation and fenced wake admission. Coordinate Core hooks
  with Secure before editing; do not duplicate the scheduler.
- Luna Acceptance: independent review and offline real-host integration tests.

Use an isolated implementation worktree rooted at the merged foundations;
serialize overlapping source changes and the shared Cargo target.

## Acceptance

- Host owns child jobs and cancellation after the owner parks. No detached
  work without a durable task/attempt/session owner.
- Carry exact root/task/attempt/parent/child/call identity across dispatch,
  verified results and wake. Invoke the actual parent secret registry before
  durable events; a caller-supplied redaction flag alone is insufficient.
- Persist finite root request/turn/time/event/retry/wake/depth/fan-out limits.
  Descendants, retries and TODO continuations cannot reset or bypass them.
  Unknown usage is visible and cannot be treated as zero under a hard quota.
- Persist dispatch intent before creating agents. Reconcile ambiguous launch,
  crash and reservation state without replaying provider or mutation calls.
- Core checklist/workflow IDs and Glass IDs use the explicit recoverable
  binding; never reinterpret the numeric workspace_task field as a Glass ID.
- Waiting/progress invoke the owner provider zero times. Two verified child
  results coalesce into at most one fenced owner continuation through the
  actual production dispatcher. Cancelled/exhausted roots dispatch nothing.
- Required success needs current evidence for every required verifier leaf;
  Settled alone, late events or checked boxes cannot close/reopen the root.

## Verification and stopping

Use fake clocks/providers with exact invocation counters and real scheduler,
child-session, runtime and checkpoint adapters. Cover duplicate/reordered
events, wrong scope, redaction, unknown usage, retry counter retention, wake
storms, cancellation, crash/reopen and uncertain dispatch. No public provider,
installation, service changes or push. Commit a bounded checkpoint, then stop
for independent review. Root merges only reviewed results; do not report an
enabled coordinator until the delegate→park→verify→single wake journey passes.
