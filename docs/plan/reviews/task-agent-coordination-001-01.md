# Task-backed coordination independent design review

Review mode: independent source and design review. No implementation was
performed.

## Scope reviewed

- Existing Core checklist, workflow, durable journal, provider budget, and child
  session boundaries.
- Glass `TaskScheduler`, budget enforcement, task-to-agent dispatch, recovery,
  and event cursor behavior.
- The proposed cross-store binding, root limits, durable inbox/wake protocol,
  cancellation/recovery contract, activity-card provenance, and standalone
  host boundary.

## Findings

All findings below are **P1 implementation blockers at the source snapshot
reviewed**: the product did not yet provide the proposed coordinator behavior.
The P1 design gives a concrete resolution for each. Subsequent source changes
are recorded after the findings so this review remains a dated evidence
snapshot rather than a claim about current implementation state.

1. **Shared budget admission is not enforced.** Core runtime loads the parent
   ledger at `crates/glass-core/src/runtime.rs:1917-24`, then independently
   loads a child ledger from the same cost file at `1951-57`. `Metered::stream`
   checks settled spending before calling the provider and charges on stream
   drop (`crates/glass-core/src/budget.rs:67-89,106-119`); it does not reserve
   concurrent requests. Parent and child usage can therefore race or overwrite
   stale ledger state. The design's single synchronized root ledger, reserve
   before dispatch, settle once, and retain uncertain reservations on crash is
   the necessary implementation contract.

2. **Checklist-to-scheduler dispatch is neither one identity nor crash-safe.**
   Core `workspace_task` is an optional numeric field and its update path stores
   that number without validating it against Glass
   (`crates/glass-core/src/todo.rs:49-59,376-414`). Glass assigns its own
   `task-*` IDs. In
   `TaskScheduler::schedule_ready`, the agent is created before the task's
   assigned-agent state is persisted (`crates/glass-dev/src/tasks.rs:1028-53`),
   leaving a crash window that can orphan or duplicate dispatch. The proposed
   versioned binding intent, idempotency key, create-or-find scheduler
   operation, and reconciliation across the two stores address this without
   claiming cross-file atomicity.

3. **Existing limits do not aggregate across a root or survive retries.**
   `max_events` is copied into a fresh Agent spec on each task attempt
   (`crates/glass-dev/src/tasks.rs:1044-45`); the scheduler's event cursor is
   initialized in memory and consumed from that cursor (`513,546,740-49`).
   Missing token usage is converted to zero by `event_tokens` (`2150-66`).
   Core's batch concurrency constant limits simultaneous children in one batch,
   not total root descendants (`crates/glass-core/src/subagent.rs:30`,
   `crates/glass-core/src/ext/protocol.rs:670-705`). The proposed finite
   root-wide request/turn/time/event/retry/wake/descendant limits, cumulative
   counters, fail-closed unknown usage, and depth/fan-out caps are required.

4. **The current child runner cannot implement the proposed park/cancel path.**
   Runtime subagent work runs synchronously, and the child calls
   `agent.run_turn(|_| {})` (`crates/glass-core/src/subagent.rs:449`), so it
   does not receive a live cancellation signal. Returning a normal “running”
   tool result would resume Core's model loop and permit polling. The proposed
   negotiated parked-turn outcome, host-owned job lease, exact child
   session/turn cancellation, and reconcile-only recovery after uncertain
   launch are necessary before enabling asynchronous coordination.

5. **At the review snapshot, live child events bypassed parent-session secret
redaction.** The child
   Agent redacts its own progress before calling the observer, but text and
   tool summary/output are emitted in `run_task_observed`
   (`crates/glass-core/src/subagent.rs:425-445`) and forwarded directly by the
   runtime (`crates/glass-core/src/runtime.rs:2012-19`). A parent-known secret
   echoed by a child is not guaranteed to be in the child's registry; final
   parent tool-result redaction happens later. The design correctly requires
   parent-session redaction at the observer boundary before publication or
   persistence, with reasoning excluded and tool-call identity retained.

6. **A configured “settled” verifier can pass with no evidence.**
   `VerificationRequirement::Settled` returns `Passed(Vec::new())`
   (`crates/glass-dev/src/tasks.rs:1979`). The P0/P1 design now explicitly
   disallows this mode from unlocking required dependencies or closing a
   verified root, and requires evidence tied to the task and attempt.

## Proposed-design verdict

**Pass for root confirmation as a proposed design; not an implementation pass.**
The design reuses the existing scheduler and child-session authorities rather
than adding a competing scheduler, and now specifies concrete cross-store
reconciliation, shared admission, unknown-usage handling, root-wide limits,
durable deduplicated inbox/wakes, cancellation, verified completion, and
redacted activity provenance. The review fixes also account for Core's
automatic continuation calls and the fact that `started`/`finished` child
events do not themselves contain task text/final report.

The proposed first-release ceilings (depth one, four active children per root,
128 root records, one retry per task, and one verified owner continuation)
remain product choices and require root/user confirmation before implementation.
P2 implementation remains gated on that confirmation and must demonstrate
provider request counts, reservation/recovery behavior, and the offline
delegate → parked wait → verified completion → at-most-one owner wake flow.

## Post-review implementation status

Commits `a426ce53` and `0e12a3ae` subsequently closed the specific live child
event redaction gap and added the parent-call-scoped Glass activity card and
read-only child navigation. These slices redact child progress before live
publication and redact child snapshot fields before preview and pagination.
They do not implement the shared root budget, cross-store task binding,
durable inbox, parked owner turn, verification gate, or bounded owner wake.
Findings 1–4 and 6 remain implementation blockers; finding 5 is closed for
the current child event and viewer paths, while a future durable inbox must
preserve the same redaction boundary before persistence.

## Verification evidence

- Installed-pair read-only viewer PTY passed with the fake Core runtime:
  `GLASS_E2E=1 GLASS_E2E_BINARY=/home/ubuntu/.local/bin/glass cargo test -p
  glass-dev --test tui_pty
  resident_subagents_view_is_read_only_live_and_preserves_main_draft_in_real_pty
  --locked -- --exact --nocapture` — 1 passed, 53 filtered. It exercised live
  child text/tool events, read-only child entries and output pagination,
  rejection of send/approval/abort input, and preservation of the main draft,
  cursor, and transcript scroll.
- Installed binaries match release targets: Glass SHA-256
  `24bedfb04d3cc0c7dee7d7926f5763edf4ace45207b5a810ce6d891be5f1bc52`,
  glass-browser `b4e3f13cc8d5bc2edd8091c3d7622d4fde281b6dcca1a7b0068a96f56aa14f1f`,
  and Core `9138306ef7be2a74d3a7f39ce27b8724be7de98b0cf35cbe7bdaf33c8bc019cb`.
  All report version 0.3.14; `glass` and `gc` resolve to their expected
  launchers/binaries. All 314 installed web assets match the source tree, and
  all five saved pre-install Core worker fingerprints remain unchanged.
- This verifies the existing viewer and installed pair only. No task-backed
  asynchronous coordinator, provider wake, live service, or public-provider
  behavior was exercised.
