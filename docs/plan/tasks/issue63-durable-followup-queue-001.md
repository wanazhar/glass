---
id: issue63-durable-followup-queue-001
scope: issue-63-durable-followup-queue
status: complete
depends-on: [issue63-durable-threads-001, issue63-core-session-resume-001, issue63-durable-composer-drafts-001]
---

# Durable per-thread follow-up queue

## Objective

Let a person prepare several follow-up messages for one resident thread, edit,
reorder, and remove them, and choose whether each message targets a concrete
active run as a steer or waits as a follow-up. Persist the queue by stable
ThreadId/RunId on Unix and keep the TUI responsive while the registry is read
or written. Never replay a persisted queue item automatically after restart.
Windows keeps process-local behavior because the registry inherits workspace
ACLs; do not read or write queue content there, and preserve any opaque field
through unrelated registry mutations.

## Data and lifecycle

- Add an optional `followUpQueue` field to each v1 thread record. An item has a
  random queue-item ID, destination RunId, explicit `steer` or `followUp`
  mode, bounded text, and delivery state. Validate RunId membership in the
  owning thread. Keep existing project identity, strict schema, serialized
  mutation, private atomic-write, and aggregate registry-size invariants.
- Queue edits only change durable local queue state. A queue row is not a
  backend-accepted message. Dispatch is an explicit user action; only a
  positive `glass.agent.send` acknowledgement may change a row to accepted.
  Persisted ready rows are displayed after restart and require a deliberate
  dispatch action; no startup path sends them. Uncertain dispatch outcomes
  remain visibly unresolved and require user review before retry.
- Steering is available only when the chosen RunId maps to the currently
  active selected run. Follow-up destination is still explicit and retained
  per item. A thread or run switch cannot silently retarget an item.
- On Unix, queue load/save uses the existing background ledger worker (or a
  coalesced extension of it), never a UI-thread lock/fsync. Keep queue state
  attached to ThreadId while changing AgentId selections. Preserve the last
  committed queue if a size check or write fails and show the failure.
- Bound queue count and aggregate serialized queue bytes within the registry
  budget. Text must be nonempty, NUL-free, and within the composer input
  limit. Keep v1 optional-field compatibility; do not promise downgrade
  preservation.

## Acceptance

- [x] User can add, edit, reorder, remove, and explicitly dispatch a queue row
      from the owning thread; steering requires an explicitly selected active
      RunId.
- [x] Restart-like reopen restores rows by ThreadId and preserves RunId/mode;
      rows are never automatically replayed.
- [x] Store validation, aggregate limits, write-failure preservation, and
      Windows opaque-field behavior have focused tests.
- [x] UI queue reads/writes are nonblocking and stale completions cannot
      replace edits made after a load began.
- [x] Focused tests, fmt, strict scoped Clippy, documentation validators, and
      an independent diff review pass; record the local Conventional Commit.

## Verification

Implemented in local `main` at `f64873e0` (`feat: add durable per-thread
follow-up queue`). The isolated implementation branch passed 16 queue-focused
tests plus the expected-RunId, NeedsReview duplicate-risk confirmation, and
cross-owner save-failure regressions. Strict scoped Clippy, formatting, and an
independent read-only diff review passed. Current documentation coverage,
depth, release-truth, and TUI-shortcut checks passed after refreshing the live
MCP schema measurement in `7e3c8a6a`. The issue remains open for the attention
inbox and broader orchestration/review journeys.
