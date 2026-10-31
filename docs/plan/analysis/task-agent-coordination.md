# Task-backed agent coordination

Status: Proposed design, not implemented. Scope: monorepo Glass and Glass Core;
the independent nightly repository is excluded.

## Objective and existing boundaries

Delegate work without model polling, update the UI live, and wake an owner only
when a bounded, actionable event needs a decision. Every execution must have a
visible stopping boundary. New user requests add work without deleting prior
requirements.

Reuse the existing [task DAG](../../task-dag.md), its verification, dependency,
retry, recovery and per-task budget machinery. Core `todo.rs` owns a session
checklist; its numeric `workspace_task` field is not a link to Glass's
`TaskId`. Core `budget.rs` meters provider usage, but the current runtime loads
separate parent and child ledgers. `subagent.rs` owns actual child sessions.
These are related authorities, not interchangeable stores. Do not create a
second scheduler or interpret a manually checked todo item as verified
execution.

## P0 source audit

The source has three distinct task-like stores, with no current cross-store
coordinator:

| Source | Current authority and boundary |
|---|---|
| Core session checklist (`crates/glass-core/src/todo.rs`, `Board`) | Session-journal `state/todo` records hold `T1`-style IDs and reported statuses. `workspace_task: Option<u32>` is just a number; the checklist UI resolves it against Core's own workflow board, not Glass's scheduler. |
| Core workflow board (`crates/glass-core/src/workspace.rs`, `Board`) | Numeric task IDs and workflow records live under Core's data directory. This is separate from both the checklist journal and Glass's project `.glass/tasks/scheduler.json`. |
| Glass project scheduler (`crates/glass-dev/src/tasks.rs`, `TaskScheduler`) | Owns `task-*` IDs, dependencies, assignment, verification and recovery checkpoint. Its 128-task cap is per workspace; each agent run has its own event/runtime budget. The event cursor is in memory and starts over when the scheduler is reopened. |

Glass has an existing verification mode that is weaker than the design's root
completion rule: `VerificationRequirement::Settled` returns
`Passed(Vec::new())` (`tasks.rs::verify`). A settled child can therefore be
marked successful without evidence. Required root dependencies must use an
evidence-producing policy; the coordinator must reject `Settled` for those
tasks or explicitly require separate root acceptance evidence before waking.

Core also has an optional durable task journal (`crates/glass-core/src/durable.rs`,
`durable_runtime.rs`; [durable runtime reference](../../../products/glass-core/docs/reference/durable.md)).
Its journal can atomically commit Core task/session/document records and expose
a change cursor. Built-in `durable/run` executes `gc.generation`; custom task
definitions are process-local and must be registered again, while
`durable/claim` leaves custom execution and checkpoints to the host. It is a
potential substrate, not an existing integration with Glass `TaskScheduler`
or a general standalone delegation worker.

The current `glass.task.wake` tool reads the latest persisted crew summary
(`crates/glass-dev/src/tools.rs`, `workspace.rs`, `tasks.rs`). It neither
starts an owner turn nor consumes a task-event inbox. Glass's follow-up queue
is for human messages to an existing run. Core runtime child events stream to
the current host, but there is no durable owner inbox/wake path. Standalone
Core does not load Glass's `DevelopmentWorkspace` or `TaskScheduler`.

The existing runtime child path is synchronous: `crates/glass-core/src/subagent.rs`
creates child sessions and, in the runtime-specific observer path,
`runtime.rs` forwards sequenced child events to the connected host. The normal
CLI callback remains synchronous. Child events are not a durable inbox, and
the current child-link sidecar is relationship metadata rather than an
acknowledged/replayable delivery log. The source audit found that the runtime
observer did not apply the parent session's `SecretRegistry` before publishing
events. That live-event gap was closed after the audit in commit `a426ce53`:
parent secrets are now applied before child event publication, and the
read-only child snapshot path redacts before preview and byte pagination.
Commit `0e12a3ae` adds the Glass activity card and read-only navigation bound to
the matching parent call and child link. These slices do not add a durable
inbox, task binding, parked owner turn, or scheduler wake; those remain
proposed work.

The budget boundary is similarly narrower than the proposed root allowance.
`crates/glass-core/src/runtime.rs` loads the parent provider ledger and a
separate child ledger from the same cost file, giving siblings a shared child
ledger but not one synchronized ledger with the parent. `budget::Metered`
checks settled spending at request start and records usage when the stream is
dropped; it does not reserve for concurrent requests. Missing usage marks cost
accounting unknown, but `crates/glass-dev/src/tasks.rs::event_tokens` maps
missing token usage to zero. Glass's current bounds therefore do not establish
a hard root-wide cost/token allowance across retries, descendants, or owner
wakes.

The design must choose one execution authority and bridge the others through
an explicit durable binding. It must not infer a Glass task from
`workspace_task`, mirror state into another scheduler, treat a streamed child
event as a durable wake, or claim shared budget admission before those paths
exist. See the [delivery task](../tasks/task-agent-coordination-001.md) for
the bounded next phases.

## State ownership

| Owner | Authoritative data |
|---|---|
| Glass `TaskScheduler` | Workspace `TaskId`, dependency state, attempts, assigned agent, verification, and execution outcome |
| Core session checklist | Session-local plan items (`T1` IDs), reported progress, revision, and optional numeric pointer into Core's workflow board |
| Execution binding | Explicit link from `(Core session, todo item, workspace)` to a Glass `TaskId` and root task; it is not a second scheduler |
| Runtime coordinator (proposed) | Durable inbox/outbox, child execution lease, event cursor, admission reservations, and owner-wake reservation; no such cross-store coordinator exists today |
| Core budget ledger | Provider usage and accounting uncertainty; parent and child runtime providers currently use separately loaded ledgers, with no concurrent admission reservation |
| Core child session | Actual child transcript, parent/turn/call linkage, and child lifecycle |
| Glass TUI / Connect web | Read-only projections, activity cards, task controls, and owner-visible stop reasons |

Project checklist intent to scheduler state without letting the checklist write
execution outcomes:

| Core checklist status | Coordination meaning | Glass scheduler authority |
|---|---|---|
| `pending` | Planned only until explicitly admitted | No Glass task is implied; an admitted task starts `queued` or `ready` according to dependencies |
| `in_progress` | A linked execution has been dispatched or is active | Scheduler owns `running`, `waiting`, and `verifying` transitions |
| `completed` | A reported checklist claim | Does not set `TaskState::Succeeded`; verifier evidence is still required |
| `blocked` | A prerequisite or human decision prevents progress | Scheduler records the corresponding blocked/waiting state and reason |
| `abandoned` | Checklist intent was withdrawn | Does not cancel dispatched work by itself; use the exact child cancellation flow and record its outcome |

This mapping is a projection, not a second transition authority. A task that
fails or is cancelled remains distinct from checklist work that was never
admitted. Root status is derived from its required task graph and acceptance
criteria: all required tasks must be verifier-backed successful for a verified
root; a terminal graph containing a failed, cancelled, or blocked requirement
is not success.

The checklist and scheduler states are not aliases. Core todo `completed` is a
reported checklist claim; Glass `TaskState::Succeeded` requires the configured
verifier to pass. `AgentStatus::Completed` and child settlement are not proof of
task success. A root is verified complete only after its required task graph
and root acceptance requirements pass. Marking a plan checkbox complete, or
finishing this planning document, does not establish that result.

Core todo IDs (`T1`, `T2`, …), Glass `TaskId` strings (`task-N`), Core session
IDs, and workspace identities remain distinct stable identifiers. Bind them
with a versioned record keyed by the source session and todo ID. Never join by
title, array position, owner text, or a parsed numeric suffix. Core's current
`workspace_task: Option<u32>` is stored by the checklist but is not a validated
Glass scheduler link; do not overload it as a `TaskId`. P0 must confirm any
compatible use before implementation. The source audit confirms there is no
direct compatible mapping today. If a link must be projected into Core state,
add an explicit opaque execution reference with a migration default of absent.

The Core session journal and Glass workspace scheduler checkpoint are separate
stores, so their writes cannot be one filesystem transaction. Create a durable,
idempotent binding intent before dispatch, keyed by `(workspace, Core session,
todo ID, todo revision, plan revision)`. The scheduler create/find operation
must accept that intent key so a crash after task creation cannot create a
duplicate. Then record the resolved `TaskId` in the binding. Recovery
reconciles an unfinished intent against both stores before it creates another
task. A failed half of this flow remains visibly pending or orphaned until
reconciled; it is never reported as an atomic link. Admission also checks
capacity in the shared workspace scheduler before dispatch; a root's proposed
128-record cap does not override the scheduler's workspace-wide cap.

Standalone GC must have an explicit host adapter. It must not depend on a
Glass workspace process, invent a second task authority, or silently run a
Glass task when no adapter is configured.

## Execution and stopping contract

An executable checklist item binds to an immutable Glass `TaskId`; each
attempt has a separate ID and each child carries its root and parent IDs.
Dependencies, acceptance requirements, and wake policy belong to the durable
task record. The root owns one inherited allowance shared by all descendants,
retries, and owner continuations. A child's effective cap is the minimum of
its request, its parent's remaining allocation, and the root's remaining
budget. Copying the full root allowance into each child is invalid.

The root envelope must bound total task records (including descendants),
delegation depth, globally active child sessions, provider requests/model
turns, elapsed time, events, retries, and owner wake turns. Proposed first
release ceilings are depth one (no recursive delegation), four active children
across one root, 128 task records across one root, one retry per task, and one
owner continuation after verified root completion. These are proposed new
root-wide caps, not claims about current enforcement: Core's four-child limit
is per batch, while Glass's 128-task cap is per workspace scheduler. Neither
currently supplies these root-wide guarantees. Per-task defaults remain 3,600
seconds and 10,000 events; a root also needs its own elapsed-time and event
totals because per-agent limits alone do not bound a DAG. Each root must supply
finite `maxProviderRequests`, `maxModelTurns`, `maxElapsed`, `maxEvents`,
`maxRetries`, and `maxOwnerWakes` before admission; there are no implicit root
defaults in this proposal. For autonomous provider dispatch, it must also
supply a hard token or cost ceiling that can be reserved before each request.
If the configured provider/model cannot supply a defensible request bound and
price, autonomous dispatch is unavailable until the human chooses a supported
budget. Never display an absent ceiling as a numeric allowance. Every retry
and owner wake consumes the same root allowance. Progress events consume event
and inbox capacity but do not trigger a model turn. A later user request
creates or explicitly extends a plan with a new revision; it does not silently
replenish the current root.

Admission reserves capacity before dispatch. Reservations are shared across
the parent, children, and concurrent requests; `remaining = limit - settled
usage - outstanding reservations`, clamped at zero. Reserve count slots as
well as usage: provider request, model-turn, child, retry, event-byte and wake
capacity is consumed before dispatch, not when a child eventually reports.
Each reservation has an idempotency key and settles or releases exactly once.
The current Core cost ledger records usage after a provider stream and does not
reserve for parallel requests; implementation must use one canonical
synchronized ledger and must not load independent stale copies for parent and
children.

Usage accounting is `known`, `unknown`, or `reconciled`; each amount also
records whether it is provider-reported or a reservation. Missing provider
usage is never converted to zero. Core's cost ledger already marks usage or
pricing gaps unknown and refuses another request when a cost limit is active;
the coordinator must preserve that fail-closed behavior across every child and
wake. Before a provider call, reserve a defensible maximum from the configured
model/output ceiling and price schedule. If an active hard ceiling cannot be
reserved, reject admission or ask the human to revise the budget; launching a
serial request without a bound does not make it safe. Settle the reservation
from reported usage, record overrun, and block further admission when a hard
limit is reached. If usage or pricing becomes unknown, no further provider
call is admitted until reconciliation or an explicit user budget decision.
Autonomous dispatch always has a hard usage limit under this proposal. A crash
with an outstanding reservation retains it until reconciliation; recovery
must not release it merely because a child process is absent. Crashes during
an in-flight request leave usage unknown until Core session accounting
reconciles it.

Reuse the scheduler's queued, ready, running, waiting, verifying, succeeded,
failed, cancelled, paused, and blocked states. Keep cancellation intent and
recovery uncertainty as persisted reasons/metadata rather than silently
changing existing serialized states. A settled model turn moves to
verification; only matching configured evidence can move a task to succeeded.
Unrelated task completion cannot close the root. A terminal task does not
reopen because a late or duplicate event arrives; a retry creates a new
attempt under the same task and root budgets and is admitted only when the prior
attempt's provider and external-tool effects are known or reconciled. An
uncertain external mutation is not automatically repeated.

For a required coordination task, a verifier must produce evidence tied to the
task and attempt. The current `VerificationRequirement::Settled` path can
return `Passed` with an empty evidence list; it is not a verification gate for
required dependencies or root acceptance. Reject that requirement for required
tasks, or treat its result as settled-only: it must not unlock dependent work,
mark a root verified, or reserve an owner wake. Empty evidence never satisfies
a required acceptance criterion.

## Durable communication and wakeups

An event carries a stable event ID plus root/task/attempt IDs, source and
recipient IDs, child session ID, monotonically increasing child sequence,
causal tool-call reference, kind, and a bounded payload. The deduplication key
is `(root, task, attempt, child session, sequence)`; retries use new attempt IDs.
Events may report assignment, progress, question, blocker, dependency-ready,
result, cancellation acknowledgement, and verification. Validate membership,
recipient, workspace, task, and attempt scope on receipt. Do not infer a
relationship from an unrelated agent row.

Redact each child text chunk and tool summary/output with the parent session's
SecretRegistry immediately before publishing or storing it. Redacting only
the final child report is too late for live events or activity cards. Exclude
reasoning, preserve tool-call identity, and cap each event and total inbox
bytes. Store a bounded summary and a reference to the real child stream, not a
second transcript copy.

The host may tick or scan its local scheduler; that is not model polling. The
host routes progress to viewers without provider requests. Progress never
wakes the owner. Questions and blockers are shown to the human and hold their
tasks; they do not start an autonomous model turn. Independent tasks may
continue while the owner is idle if they do not depend on the blocked decision
and still fit the root envelope. Persist verified completion events and
coalesce them under one inbox watermark and wake ID. The first rollout permits
at most one autonomous owner continuation, and only when every required task
is verifier-backed successful and the root acceptance criteria are satisfied.
If a required task fails, is cancelled, or remains blocked, surface the stop
reason to the human without automatically waking the model. A human request or
explicit revised plan is required for another execution wave.

Persist event and wake intent before delivery; acknowledge only after the
disposition is durable. Deliver each child session/attempt's events in sequence
or buffer bounded out-of-order events; a persistent gap or full reorder buffer
sets a visible recovery-needed reason rather than silently advancing the
watermark. Replay is at-least-once and deduplicated, not exactly once for
provider or tool side effects. A single fenced turn lease per Core session
prevents concurrent owner turns. Bind it to the exact session and turn ID;
lease expiry alone is not proof that an old turn stopped. If ownership cannot
be reconciled after a crash, do not issue another model request. If the owner
is busy, keep events queued for the next safe boundary. The wake carries the
bounded inbox batch and consumes the root's remaining provider, token/cost,
runtime, and wake allowance. A wake cannot trigger an unlimited wake-on-wake
loop.

Core's TODO auto-continue can issue additional model calls from one Agent
turn. Disable that behavior for coordinated roots in the first rollout so one
owner wake cannot silently expand into extra turns. If it is enabled later,
every continuation must reserve and consume the same root-wide
`maxProviderRequests` and `maxModelTurns`, retain the fenced owner-session
lease, and stop when either limit is exhausted. Do not count a whole
auto-continue chain as one request or one wake.

The current synchronous subagent callback must be migrated deliberately to
host-owned jobs. The first implementation target is delegate, park the owner
turn without a provider request, persist a completion event, verify it, then
reserve one owner continuation. A normal tool result saying “running” is not
wait: Core would resume its model loop and invite polling. Add an explicit,
negotiated deferred/parked-turn outcome that records the pending call and lets
the owner turn settle without another model request; only the durable wake may
resume it. Keep the current synchronous contract for hosts that do not
negotiate this capability. A Core session without a durable-job host adapter
must report delegation unavailable rather than start an untracked child. Do
not detach a thread that loses task, cancellation, transcript, or turn
ownership.

## Recovery, controls and privacy

Persist task bindings, inbox cursors, dedup keys, attempts, reservations,
cancellation intent, and wake reservation in a versioned bounded checkpoint
before dispatch or acknowledgement. On restart, reconcile a child against its
Core link/session and durable usage before resuming. An accepted dispatch with
unknown effects is not replayed. The existing scheduler's recovery-pending
state is the default for an assigned task whose prior outcome is unknown; it
requires inspection and an explicit resume/new attempt. A missing child or
incomplete provider record never means “not started” or zero spend.

Cancellation is a persisted intent sent to the exact child session and turn.
Keep cancellation intent separate from the scheduler's existing serialized
state vocabulary. Mark cancelled only after Core confirms cancellation. If
the child instead settled normally, preserve its result and run verification.
If acknowledgement is missing, keep the task paused/recovery-pending with an
unknown outcome and block retry until reconciliation. Do not claim an external
tool mutation was undone. Viewer close is read-only and does not cancel a task
or start a model turn. Budget extension and retry are explicit,
revision-checked actions; they never erase charges or previous attempts. A
retry gets a fresh attempt ID and is admitted only when the prior attempt's
provider and external-tool effects are known or reconciled. An uncertain
external mutation is not automatically repeated.

The root is bound to its originating Core session and workspace. Closing a
viewer does not stop work; ending or deleting the owner session stops future
owner wakes. Keep task and child records available under normal retention so a
human can inspect and explicitly resume from a new session. Do not silently
rebind the old root to a new session or start another owner turn. If a host
cannot retain the child session/transcript for the configured retention period,
it must expose that as a stop reason before dispatch.

Task records, inbox payloads, event histories, and summaries have separate
finite byte and item caps. Under capacity pressure, preserve task-state
transitions, sequence watermarks, usage status, and terminal reasons while
dropping or compressing older descriptive payloads. Corrupt or over-limit
checkpoints fail closed; they do not start an empty scheduler or replay work.
Tool-output details retain call identity and use existing scoped read APIs.
Arbitrary agents cannot message another project's task or gain authority from
an inbox event or viewer.

## User interface

Use existing task/checklist views and `/subagents`; do not add another competing
dashboard. Preserve main draft, cursor, scroll and child read-only behavior.

```text
Tasks                         2 verified / 4 required
  Build parser      Luna      Running       8 / 20 turns
  Verify parser     Reviewer  Waiting       depends on Build
  Integrate         Main      Waiting       wakes on verification

Main activity
  Assigned · Luna · Build parser             [details]
  Completed · Luna · evidence ready          [open child]
  Owner waiting · no model requests          [task limits]

Watching Luna · Read only
  live child conversation and tool cards
  Budget: 8 / 20 turns · 1 / 2 retries · elapsed 4m / 15m
```

The displayed figures are example values, not proposed defaults; show the
actual root envelope and mark unknown accounting as unknown.

Activity cards show real assignments, progress and results, not invented
inter-agent dialogue. Expanding a card shows the actual bounded exchange;
opening a child shows its existing full read-only stream. Display stop reason,
verification state, remaining limits and unknown accounting explicitly.
Bind an assignment card to the child's actual initial User record and the
validated task link. Bind a terminal card summary only to the authenticated
parent `run_subagents` result matched by `(parentCallId, taskIndex)`; a
`started` event has no task text and a `finished` event has no report text.
Render child progress as attributed child activity. Never synthesize a child
message from an unrelated agent row or treat an unauthenticated event as a
task assignment/result.

## Required integration paths

1. Checklist state creates a versioned binding intent; recovery joins the Core
   session/checklist and Glass task stores without a cross-file atomicity claim.
2. Admission reserves shared root capacity before the scheduler creates or
   attaches the Core-owned child execution.
3. The owner turn parks without another provider request; waiting and routine
   progress are host-only operations.
4. Child progress is redacted and bounded before event publication and durable
   inbox/card storage; it updates viewers without an owner provider request.
5. A terminal child result is deduplicated into the inbox and routed through
   the configured verifier before the task state changes to succeeded.
6. Verified prerequisites unlock the existing DAG. Actionable terminal events
   coalesce under one durable wake reservation and the single owner turn lease.
7. The owner consumes one bounded inbox batch, records disposition, and cannot
   trigger an unbounded new child/wake loop.
8. TUI and web project the same task, limits, stop reasons, binding and inbox
   watermark; they do not own execution state.
9. Crash recovery restores bindings, reservations and cursors without
   replaying provider calls or uncertain tool work.

## Verification and rollout

Use fake-clock/provider deterministic tests plus real host integration with an
offline child runner. Count provider invocations: zero during park, wait, and
progress; at most one coalesced continuation for the first root completion.
Exercise binding crashes at each store boundary, duplicate and out-of-order
events, inbox overflow, crashes before/after acknowledgement, concurrent
reservations, unknown usage with hard limits, reservation overruns, scope
rejection, cancellation without acknowledgement, retry without counter reset,
stale wake leases, malicious wake loops, and a completed checkbox lacking
verification evidence.

Verify a real TUI and web path: create checklist, delegate two dependencies,
watch children read-only, let owner wait without polling, complete and verify,
observe one owner continuation, then terminal root with no more requests.
No paid/live provider is required for this acceptance.

Deliver in phases from the linked task checklist. Proposed design must be
reviewed and confirmed before production implementation. No live worker,
Tailscale route, provider configuration or nightly repository changes belong to
this planning task.
