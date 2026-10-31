# Independent review: durable task inbox and wake reservations

Status: **pass for the bounded scheduler foundation**. This review covers
`f63d462a` on the isolated `task-agent-inbox-003` branch. It does not certify
an end-to-end Core-to-Glass coordinator or enable automatic owner wakeups.

## Findings

- **P2, integration prerequisite — parent-secret redaction is asserted, not
  enforced by this scheduler API.** `TaskInboxEvent` requires
  `parent_redaction_applied`, but that Boolean is supplied by the caller. The
  event is size-bounded before persistence; the scheduler cannot verify that a
  parent Core `SecretVault` redacted it. A future Core adapter must apply the
  parent session redactor before constructing or persisting events, and must
  not treat the Boolean as proof of sanitization.
- **P2, integration prerequisite — no production wake dispatcher exists in this
  slice.** `OwnerWakeDispatchAdapter` is an offline-testable seam only. No Core
  runtime or provider is invoked, so these checks validate durable binding,
  inbox, event-budget, verification and wake-lease behavior, not an actual
  delegated child causing an owner turn.

No blocking source findings remain for the bounded scheduler foundation.

## Review evidence

- Cross-store task bindings persist an idempotent intent before resolution and
  retain the resolved task/root/attempt/child-session/parent-call identity.
  Retry advancement archives prior resolved identities in a bounded history;
  old event rows are validated against that history on reopen, and late
  redelivery from a prior attempt is classified stale. Tampering with an old
  event identity is rejected during checkpoint loading.
- Required task events are bound to the configured owner session, current root,
  required task, attempt, child session and parent call. Late policy
  registration also checks historical required-task bindings, preventing a
  policy from persisting successfully and then failing checkpoint validation.
- Verified completion requires current-attempt task evidence. `Settled` and
  `Inferred` leaves cannot satisfy an evidence-producing requirement; an
  `All` requirement needs evidence for each evidence-producing leaf. A root
  wake is reserved only after all required completion receipts and matching
  bindings exist.
- Inbox append is persisted before it returns an acknowledgement result.
  Duplicate IDs, conflicting reuse, out-of-order buffering, stale attempts,
  global count/byte bounds, per-event bounds, root policy counters and unknown
  wake outcomes fail closed. An unknown dispatch remains reconciliation-only;
  it is not automatically replayed. The owner session has one fenced active
  wake lease, and wake reservation is capped at one.
- Scheduler state uses the existing `TaskScheduler` checkpoint and real reopen
  path. The tests do not substitute an in-memory mock scheduler. No provider,
  service, install, or push was used.

## Validation reported for the frozen commit

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib tasks::tests:: --locked` — 39 passed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-dev --lib task_inbox_global_event_cap_persists_stop_reason_without_wake --locked` — 1 passed. It accepted 256 persisted events, rejected event 257, reopened with the persisted stop reason and count, and observed zero wake adapter calls.
- `cargo fmt --all -- --check` — passed.
- `git diff --check` — passed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo clippy -p glass-dev --lib --tests --locked -- -D warnings` — passed.

The implementation has no Core task-binding adapter, production inbox source,
provider wake dispatcher, or owner-turn integration. Do not report these
foundations as an enabled end-to-end feature.
