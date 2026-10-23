# Autonomous task DAGs

**Status: Current source behavior.** The task scheduler assigns work to
resident agents and checks their results. It is not a general workflow engine.
Task checkpoints persist under `.glass/tasks/scheduler.json`; active assigned
work reopens as recovery-pending and is never automatically replayed.

```text
TaskSpec (goal, prompt, policy, verifier, budget)
       │
       ▼
TaskScheduler ── dependency and retry state
       │
       ▼
AgentRegistry ── one AgentSpec / worker / selected resident engine
       │
       ▼
agent turn ── bounded events and attributed evidence
       │
       ▼
verification ── command or governed resident evidence
```

`DevelopmentWorkspace` owns one `TaskScheduler` and one `AgentRegistry`. The
daemon checks the scheduler every 50 ms, even when no client is connected.
The scheduler atomically checkpoints task records below the project root.
Agent session files and timeline evidence have separate persistence rules;
the checkpoint does not claim that an interrupted engine turn has been
recovered. Glass Core is the sole resident engine. See [Development Runtime](development-runtime.md),
[Glass Core resident harness](glass-core-harness.md), and [Local daemon](daemon.md).

## Entry points

The TUI exposes `:task` commands. MCP and resident agents use these tool names:

```text
glass.task.create          glass.task.list
 glass.task.get            glass.task.inspect
 glass.task.pause          glass.task.resume
 glass.task.cancel         glass.task.retry
 glass.task.reassign       glass.task.override-blocked
 glass.task.evidence       glass.task.verify
 glass.task.crew           glass.task.wake
```

## Overnight factory crew and wake packs

`glass.task.crew` queues a bounded architect, isolated implementer/tester
pair(s), reviewer, and browser-verification DAG. The workspace records a
`before-crew:<goal>` editor checkpoint and prepares paths below
`.glass/worktrees`; Git worktrees are used when available and confined
directories are used as the fallback. The crew is workspace-scoped and
retains task-level verification and evidence records.

The scheduler's task records checkpoint under `.glass/tasks/scheduler.json`;
the agent-event cursor belongs to the live registry and restarts with it. The
crew summary has a separate durable record: Glass writes
`.glass/crew/<crew-id>.json` and replaces
`.glass/crew/latest.json` with the latest `CrewWake` record. `glass.task.wake`
is read-only and returns that latest record. A wake refresh folds member states,
bounded Git diff and test output, plus live verify, page, and proposal-accept
evidence into the persisted summary. The TUI exposes the same operations as
`:task crew GOAL` and `:task wake`.

Crew wakes are distinct from `glass.todo.*`: the workspace-local Agent
checklist is persisted at `.glass/todos/session.json`, while a crew is an
overnight factory orchestration record.

There is no standalone `glass task ...` scheduling CLI family in the current command parser. `glass task validate/compile` is a separate browser-free Task Protocol feature and does not create scheduler records. Use the TUI or a workspace router client:

```text
:task create TITLE PROMPT
:task create-after TASK-ID TITLE PROMPT
:task list
:task inspect TASK-ID
:task pause TASK-ID
:task resume TASK-ID
:task cancel TASK-ID
:task retry TASK-ID
:task reassign TASK-ID ROLE [MODEL] [THINKING]
:task override TASK-ID
:task evidence TASK-ID KIND true|false [JSON]
```

Task creation, control, and evidence use the shared workspace router. Calls
retain actor attribution, workspace-generation and project-revision checks,
and the task's declared verification requirements. Glass Core owns resident
coding tools and their permissions; task scheduling records work and evidence
without adding a second coding permission policy.

## Lifecycle and dependency wakeup

A new task is inserted as `queued`. It becomes `ready` when all dependencies are `succeeded` and an agent is allocated. The scheduler sends the full task prompt when the assigned agent emits `ready`, then moves the task to `running`. A settled agent turn moves it to `verifying`; verification can finish or wait for externally collected evidence.

```text
queued ── deps satisfied ──► ready ── agent ready/prompt ──► running
  ▲                                                        │
  │                                                        ▼
  │             retry/backoff ◄── failed ◄── verifying ◄── settled
  │                                  │                    │
  └──────────── retry action ────────┘                    └─ waiting

queued/active ── pause ──► paused ── resume ──► queued
queued/active ── cancel ──► cancelled
failed/cancelled dependency ──► blocked descendants
blocked ── override-blocked ──► queued (explicit human override)
```

Terminal task states are `succeeded`, `failed`, and `cancelled`. `paused` and `blocked` retain their reason. A failed or cancelled prerequisite never becomes success by inference. `override-blocked` deliberately allows the task to queue without requiring its failed dependency to succeed; record why this exception is safe before continuing.

The task snapshot includes ID, title, goal, full prompt, role, dependencies,
assigned agent, worktree, model, thinking, budget,
verification requirement, retry policy, state, attempt, observed tokens,
timestamps, last error, bounded evidence, override flag, and
`recoveryPending` and `evidenceTruncated`. IDs use the `task-` prefix and are
capped at 64 bytes. `evidenceTruncated` indicates that the bounded evidence
history has been pruned or compressed.

## Verification policies

Agent settlement proves that the selected engine ended the turn. It does not
prove that the agent completed the task. The default `inferred` policy resolves
when the task is created:

| Project evidence at creation | Inferred verifier |
|---|---|
| `Cargo.lock` | `cargo test --workspace --all-targets --locked`, expected exit `0`, 600-second timeout |
| `Cargo.toml` without lockfile | `cargo test --workspace --all-targets`, expected exit `0`, 600-second timeout |
| `package.json` | `npm test`, expected exit `0`, 600-second timeout |
| `pyproject.toml` or `pytest.ini` | `python -m pytest`, expected exit `0`, 600-second timeout |
| `go.mod` | `go test ./...`, expected exit `0`, 600-second timeout |
| No marker | Git change required (`hasChanges: true`) |

Specify `settled` only when deliberately accepting a completed agent turn without deterministic proof. Other requirements are `command`, `lspDiagnostics`, `browserWorkflow`, `semanticRegression`, `debuggerAssertion`, `gitChange`, `trustedCustom`, and bounded `all` compositions. `all` waits for every child requirement and preserves each child evidence record.

Command verification runs `sh -lc` on Unix or `cmd.exe /d /s /c` on Windows in the task worktree. It suppresses command stdout/stderr and records expected and actual exit codes. A non-zero exit, missing executable, or timeout fails verification. Resident LSP, browser/workflow, semantic, debugger, Git, and custom evidence is accepted only through its governed service path and must match the requirement fields. A bare agent claim such as “tests passed” does not satisfy a requirement.

While waiting for evidence, the task is `waiting`. `DevelopmentWorkspace::tasks()` collects resident verification for waiting tasks and moves them to `verifying` when evidence is submitted. Evidence includes kind, actor, source, pass flag, observation time, and bounded details. The newest matching evidence is evaluated.

## Budgets and limits

| Boundary | Current limit/default |
|---|---|
| Task records per scheduler | 128 |
| Dependencies per task | 32 unique IDs |
| Task title, goal, prompt, and role | 128 KiB each maximum; non-empty and no NUL |
| Retained evidence per task | 128 values; overflow removes older entries and marks `evidenceTruncated` |
| Persisted failure detail | 160 bytes maximum; long text is sanitized and truncated, and may be omitted if needed to preserve the state transition |
| Verification command | 16 KiB; timeout 1–600 seconds |
| `all` verification group | 1–16 requirements; nesting depth ≤8 |
| Task runtime budget | 3,600 seconds by default; must be positive |
| Task event budget | 10,000 events by default; must be positive |
| Token budget | Optional; failure when observed tokens reach the limit |
| Agent capacity | 32 resident agents; task records also consume an agent while assigned |

A task worktree must canonicalize successfully. The root or a Git worktree is accepted; `/` and unrelated non-Git paths are rejected. The task scheduler does not itself create or delete Git worktrees. Use [Experiments](experiments.md) or Git tools to prepare a worktree before task creation.

## Retry, failure, and recovery

`RetryPolicy.max_retries` defaults to `0`; `backoff_seconds` defaults to `1`. The first dispatch is attempt `1`. A verification failure retries while `attempt <= max_retries`, then becomes terminal `failed`. For example, `max_retries: 1` permits attempts 1 and 2. Backoff is checked on scheduler refresh; it is not a separate durable timer.

| Failure or action | State/effect | Recovery |
|---|---|---|
| Dependency fails or is cancelled | Descendants become `blocked` with a prerequisite reason | Fix or retry the prerequisite, then retry the blocked task, or explicitly override it |
| Agent worker fails, is cancelled, or exits before settle | Task becomes `failed` with `lastError` | Inspect agent evidence, retry task, or reassign role/model/thinking |
| Runtime, event, or token budget exceeded | Assigned agent is cancelled and task fails | Increase the relevant bound only when justified, then retry |
| Verification command exits unexpectedly | Evidence records actual exit; task retries or fails | Inspect the worktree and command, then retry with a corrected verifier |
| Verification command times out | Child is killed; task retries or fails | Use a bounded command or larger timeout ≤600 seconds |
| Resident evidence missing | Task remains `waiting` | Run the governed LSP/browser/debugger/Git/custom operation and submit matching evidence |
| Task paused | Assigned agent is cancelled; task becomes `paused` | Resume to queue a new attempt |
| Task cancelled | Assigned agent is cancelled; descendants become blocked | Create a new task or retry only if cancellation was intentional and state is safe |
| Scheduler/daemon closes with queued, unassigned tasks | Queued state and stable IDs persist | Dependencies are checked and safe queued work may be scheduled on refresh |
| Scheduler/daemon closes with an assigned task | Assignment, state, prompt, and evidence persist; on reopen the task is paused with `recoveryPending: true`, which marks the prior outcome unknown. An explanatory `lastError` is retained when checkpoint capacity permits. | Inspect the prior agent/session evidence. Resume is explicit and starts a new attempt; it clears the prior assignment from the active task record |
| Checkpoint is malformed, unsupported, or over 2 MiB | Workspace scheduler initialization returns an error; it does not silently replace the checkpoint with an empty task set | Preserve and inspect the checkpoint before repair |

A retry resets assignment, completion time, last error, and retry deadline. It does not erase bounded evidence or reset the task ID. `reassign` cancels a current agent, changes role/model/thinking, and queues the task again. Terminal tasks cannot be paused or reassigned. `recoveryPending` preserves the prior assigned agent ID as historical context, but the scheduler ignores that assignment for event correlation. Explicit `resume` or `reassign` clears the marker and creates a new assignment; neither action proves what the prior engine turn did.

The checkpoint is version 1 JSON and bound to the canonical project root,
limited to 2 MiB and 128 tasks, and is
replaced through a same-directory temporary file, file sync, atomic rename,
and directory sync. On Unix, `.glass/tasks` is mode `0700` and the checkpoint
is mode `0600`. A missing checkpoint starts a new empty scheduler; an
unsupported version or invalid/boundedness-violating record fails startup so
existing task history is not silently discarded. The checkpoint stores task
prompts and evidence locally in the project state directory.

Each task retains at most 128 evidence entries. If an incoming runtime event
would exceed the aggregate checkpoint bound, the scheduler first evicts older
duplicate evidence globally in deterministic timestamp/task order, preserving
the newest entry for each evidence kind on each task. It then compresses the
incoming event to verifier-relevant scalar fields and, only if needed, omits
that event's evidence while retaining its task-state transition. Event payloads
and repeated evidence are therefore bounded history, not an unlimited audit
log.

## State ownership and security

The assigned agent receives the task prompt, worktree, model, thinking level,
and budgets. Glass Core owns the agent session and native coding permissions.
The scheduler owns task state and does not
bypass the agent tool gateway. Every tool call records its actor and checks
the current project or browser revision. File operations stay inside the
project root. Browser mutations use the current browser lease. Glass
integrations retain operation-specific confirmations. Task evidence records
its source. An agent claim cannot
replace missing or stale evidence.

Task options and budgets are sent to the assigned Core session. They do not
replace Core's permission decisions or remove Glass integration path, revision,
protocol, process, or browser policy checks.

## Verification and evidence

Scheduler tests use a fake agent backend and do not consume a model request:

- `tasks::tests::tasks_dispatch_prompts_verify_and_wake_dag_dependents` checks dispatch, settlement, verification, and dependency wakeup.
- `eight_ready_tasks_dispatch_before_integration_dependency_wakes` checks parallel ready leaves and a dependent integration task.
- `verification_failure_retries_then_blocks_descendants` checks retry count, terminal failure, and descendant blocking.
- `external_verification_evidence_is_proof_not_agent_claim` checks waiting state and matching external evidence.
- `implementation_tasks_infer_project_verification_and_settle_only_is_explicit` checks project-marker inference and explicit settle-only policy.
- `task_checkpoint_restores_identity_evidence_and_requires_explicit_recovery` checks stable IDs, assignment and evidence restoration, no prompt replay on reopen, and one explicit resume dispatch.
- `task_checkpoint_rejects_unknown_versions_and_oversized_files` checks strict checkpoint version and byte bounds.
- `task_checkpoint_is_bound_to_project_and_preserves_the_next_id` rejects copied checkpoints and ID reuse; `task_checkpoint_rejects_symlinked_state_paths` keeps state within the project-local directory.
- `task_checkpoint_revalidates_worktree_and_verification_policy` checks restored task policy before accepting a checkpoint; `recovery_pending_controls_tolerate_missing_historical_agents` covers explicit reassign/cancel after a restart.
- `checkpoint_capacity_rejects_task_and_evidence_before_mutation_or_dispatch` checks aggregate-capacity preflight; `oversized_runtime_event_is_summarized_and_cursor_advances_after_checkpoint` and `checkpoint_saturation_drops_old_event_evidence_and_advances_cursor` check bounded evidence and safe refresh under saturation across two tasks.
- `oversized_event_summary_only_copies_bounded_verifier_scalars` checks that oversized values do not survive in the summary under verifier-field names.

The daemon workspace actor test `daemon::tests::workspace_actors_do_not_serialize_unrelated_long_operations` checks that a one-second operation in one workspace does not block another actor. The daemon tick is 50 ms with missed ticks skipped. Run these targeted tests when changing scheduler behavior; keep release claims tied to [release evidence](release-evidence.md).

## Related guides

- [Development Runtime](development-runtime.md)
- [Glass Core resident harness](glass-core-harness.md)
- [Coding harness architecture](harness-architecture.md)
- [Development Graph and replay](development-graph.md)
- [Experiments](experiments.md)
- [Local daemon](daemon.md)
- [MCP integration](mcp.md)
