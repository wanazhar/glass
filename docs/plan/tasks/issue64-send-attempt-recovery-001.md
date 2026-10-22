---
id: issue64-send-attempt-recovery-001
scope: issue-64-durable-send-attempt-recovery
status: in-progress
depends-on: [issue63-run-phase-recovery-001, issue63-attention-inbox-001]
---

# Durable send attempt recovery

## Objective

Persist bounded per-message delivery state so a reopened project can distinguish
an unresolved send from a confirmed engine response. Never resend a request
automatically or label a run complete based only on a queued command, a wait,
or a run-level settled event.

## Context

- [Glass delivery plan](../README.md)
- [Durable worker phase recovery](issue63-run-phase-recovery-001.md)
- [Durable thread/session recovery](issue63-core-session-resume-001.md)
- [Attention inbox](issue63-attention-inbox-001.md)
- [Development Runtime](../../development-runtime.md)

`RunId` identifies a worker incarnation. A separate attempt ID identifies each
submitted prompt, follow-up, or steering request. Runtime request IDs correlate
engine responses to the attempt that issued them. Run-level events without an
attempt identity may update an attempt only when exactly one active attempt
matches that run.

## Paths

- `crates/glass-dev/src/thread_store.rs`
- `crates/glass-dev/src/agents.rs`
- `crates/glass-dev/src/tui/state.rs`
- `docs/development-runtime.md`
- `docs/architecture/development-tui.md`
- `docs/plan/README.md`

## Data and lifecycle

- Store bounded attempt ID, client message ID, target RunId, mode, message text,
  status, engine request ID, and creation time in the project thread ledger.
- Create the `dispatching` record before queueing a worker command. Record a
  local queue failure as `failed`; record the engine request ID only after
  dispatch is acknowledged.
- Correlate an engine JSON-RPC response by its exact request ID. A successful
  response settles that send attempt; an error fails that attempt. Run-level
  `agent_settled` alone does not settle messages.
- On Unix, reopening converts unresolved dispatching/accepted/working/waiting
  attempts to `uncertain`. Show the exact attempt in the attention inbox and
  require inspection before any retry. Never automatically replay it.
- On platforms where the ledger does not have private ACL handling, keep
  message text and send-attempt state process-local. Preserve any opaque
  serialized fields through unrelated registry mutations.
- Enforce per-thread count, aggregate text, field, ID, and valid transition
  bounds within the existing registry size limit. When space is needed, prune
  settled/failed records before new sends; never prune unresolved attempts. If
  unresolved attempts fill the bound, reject another send with an explicit
  storage-limit error rather than lose the recovery evidence.

## Acceptance

- [ ] Every user prompt, follow-up, and steer has a stable client message ID and
      a unique persisted attempt ID before dispatch on supported hosts.
- [ ] Queue rejection and runtime errors update only the matching attempt;
      successful responses use exact engine request-ID correlation.
- [ ] Run-level wait/settled events never falsely settle an ambiguous attempt.
- [ ] Reopen converts unresolved attempts to uncertain, exposes them in the
      owning thread's attention context, and never resends them.
- [ ] Retry can reuse a client message ID while receiving a new attempt ID.
- [ ] Bounds, invalid transitions, response correlation, reopen behavior,
      platform privacy behavior, and attention selection have focused tests.
- [ ] Relevant package tests, formatting, strict scoped Clippy, docs coverage,
      depth, links, and diff review pass.

## Evidence

Current implementation is uncommitted on local `main`; the source checkpoint
before this task is `aa17506f`. Store attempt, lifecycle, exact-session reopen,
and attention-inbox focused tests currently pass. A review and final task-level
verification remain before marking this task complete.
