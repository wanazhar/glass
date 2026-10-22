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

- [x] Every user prompt, follow-up, and steer has a stable client message ID and
      a unique persisted attempt ID before dispatch on supported hosts.
- [ ] Queue rejection and runtime errors update only the matching attempt;
      successful responses use exact engine request-ID correlation.
- [x] Run-level wait/settled events never falsely settle an ambiguous attempt.
- [x] Reopen converts unresolved attempts to uncertain, exposes them in the
      owning thread's attention context, and never resends them.
- [x] Retry can reuse a client message ID while receiving a new attempt ID.
- [x] Abrupt Glass exit during a live response reopens as uncertain without
      replaying the prompt.
- [x] Bounds, invalid transitions, response correlation, reopen behavior,
      platform privacy behavior, and attention selection have focused tests.
- [ ] Relevant package tests, formatting, strict scoped Clippy, docs coverage,
      depth, links, and diff review pass.

## Evidence

The store and agent lifecycle implementation is recorded in
`c2610d3f` (`feat: persist resident send attempts`), with Windows opaque-field
preservation in `a8d923a3`. Store tests cover transition bounds, reopen
classification, and pruning; agent tests cover pre-dispatch persistence,
stable client IDs, unique attempt IDs, exact engine request correlation, and
run-level events that must not settle an ambiguous attempt.

`uncertain_send_opens_attention_without_replaying_after_restart` now exercises
the release TUI with a persisted unresolved send at 80×24 and 50×16. Reopening
classifies it as uncertain, the attention item exposes its message and routes
to the owning conversation with inspect-before-retry guidance, the IDs and
single attempt remain unchanged, and the fake Core request log records no
`turn/prompt` replay. This scenario starts from an unresolved ledger record;
it does not kill Core during a live response.

`uncertain_send_retry_requires_confirmation_and_reuses_client_message_id`
exercises explicit retry after exact-session resume. It first opens the retry
confirmation and cancels, proving no new attempt or prompt request is created.
It then confirms, checks that fake Core receives the stored text, and verifies
the original uncertain attempt remains while a second settled attempt uses the
same client message ID and a different attempt ID. The implementation
revalidates ownership, uncertain status, and current run before sending. The
`live_core_response_crash_reopens_as_uncertain_without_replay` adds the live
interruption path. The fake Core holds an active prompt before its response;
the PTY journey verifies the ledger is `working`, terminates Glass during that
window, and restarts it. The reopened attention inbox routes to the owner,
startup does not send another prompt, and the retained record becomes
`uncertain` with its original message. This is a process-interruption test with
the offline runtime fixture, not a provider-backed timing test.

The lifecycle unit test also registers a steer with a stable client ID and
unique dispatching attempt ID, then applies a runtime failure to that attempt
and verifies the two unrelated settled attempts remain unchanged. The remaining
unchecked criteria cover queue-full rejection behavior and the task-level final
package/documentation review.
