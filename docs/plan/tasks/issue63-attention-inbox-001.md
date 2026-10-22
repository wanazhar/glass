---
id: issue63-attention-inbox-001
scope: issue-63-attention-inbox
status: complete
depends-on: [issue63-durable-followup-queue-001]
---

# Attention inbox

## Objective

Give the operator one keyboard-driven place to inspect agent approvals,
pending editor proposals, and unresolved follow-up queue entries, then navigate
to the exact item that needs attention.

## Acceptance

- [x] The inbox gathers approvals, editor proposals, and unresolved follow-up
      entries into stable selectable items.
- [x] Refresh preserves selection where possible and does not steal composer
      focus or automatically retry work.
- [x] Activating an editor proposal opens its owning file/proposal context.
- [x] An allocated-PTY keyboard journey verifies inbox selection, return to the
      composer after asynchronous refresh, and proposal context at 80×24 and
      50×16.
- [x] Focused tests, scoped strict Clippy, formatting, and review pass.

## Evidence

- Inbox implementation: commit `46be3c80`.
- Allocated-PTY journey: commit `9059c686`.
- Focused PTY invocation and results are recorded in the issue #63 progress
  thread. The journey verifies typing after the inbox closes; it does not claim
  to observe a hidden background refresh while the composer remains active.
