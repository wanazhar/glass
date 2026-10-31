---
id: task-agent-inbox-003
scope: Glass task binding and durable coordination inbox
status: ready
depends-on: [task-agent-coordination-001]
---

# Task binding and durable inbox

## Objective

Extend existing Glass task/state authorities with recoverable checklist links,
durable bounded events, deduplication and one fenced owner-wake reservation.
Do not implement a competing scheduler or launch providers in this slice.

## Context

Read the [design](../analysis/task-agent-coordination.md), P0 authority mapping,
P1 reconciliation/wakeup contracts, and the independent design review. Core
todo numeric workflow IDs and Glass `task-*` IDs are different identities.

## Path

Luna Capture owns Glass `tasks.rs` and narrowly related scheduler persistence
modules/tests. A focused coordinator module is acceptable only as an adapter to
the existing scheduler. Do not change Core runtime/budget or TUI production
files; agree shared serialized contracts with Luna Secure before coding.

## Acceptance

- Versioned binding includes Core session/todo ID, workspace/task/root/attempt;
  cross-store intent and reconciliation do not claim atomic multi-file writes.
- Events carry verified root/task/attempt/child identity, stable event ID and
  sequence; redact before persistence and bound payload and queue sizes.
- Duplicate, reordered and late events cannot double-charge or reopen terminal
  work. Persist before acknowledgement; tolerate redelivery after restart.
- Progress updates a projection without a provider call. Coalesce actionable
  verified completions into one bounded wake lease; no concurrent owner turns.
- Unknown launch/reservation outcomes require reconciliation, not tool replay.
- Required success needs nonempty task/attempt-bound verification; `Settled`
  alone cannot unlock a required root/dependency or trigger its success wake.
- Retry/cancel/recovery preserve cumulative counters and explicit stop reason.

## Verification and stopping

Use fake clocks and an offline dispatch adapter. Inject crashes before/after
binding, event append, acknowledgement and wake reservation. Assert zero model
requests during progress/wait, one wake reservation for sibling completions,
wrong-parent rejection and no automatic restart after terminal state. Test
against real scheduler persistence, not only an in-memory duplicate model.
Run focused tests, formatting and strict Clippy. Commit for independent review;
no provider, installation, live services or push. One development/review cycle,
then report results and the exact integration prerequisites for P2 continuation.


Independent acceptance review: [task-agent-inbox-003-01](../reviews/task-agent-inbox-003-01.md).

Independent acceptance review: [task-agent-inbox-003-01](../reviews/task-agent-inbox-003-01.md).
