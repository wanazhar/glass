# Execution lifecycle consolidation

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../INDEX.md) for supported user behavior.

Date: 2026-10-13. Source audit of planning, code review, advisor, one-shot
delegation and persistent teammates. All execute through `agent::Agent`.

| Entry | Context and owner | Permissions | Accounting | Cancellation and persistence |
| --- | --- | --- | --- | --- |
| Main planning | Existing foreground session, project guidance, attachments | `with_plan_mode(true)` selects inspection schemas and rejects writes | Foreground metered factory and ledger | Foreground stream pump; saved plan requires explicit execution |
| Code review | Explicit diff task, fresh scratch session via `subagent::run_task` | Always read-only | Parent factory meter; separate delegated transcript | Synchronous batch; no persistent teammate lifecycle |
| One-shot delegate | Fresh session/state; optional isolated worktree; persona or raw base prompt | Per-task read-only flag; normal enforcement remains active | Caller supplies metered provider, including model override | Synchronous wave scheduler; failure isolated to its result slot |
| Persistent teammate | Durable session/state, queued follow-ups, selected workspace/model | Stored read-only flag | Per-worker reported/estimated usage, reservation reconciliation | Pumped stream cancellation, parked transcript and explicit follow-up |
| Advisor | Opt-in cadence or explicit request delegates a review checkpoint to the existing team hub | Configured read-only reviewer, no writing reviewer fallback | Same teammate accounting, exposed separately | Existing team ownership; unchanged-patch deduplication |

## Shared setup

`Agent::with_task_context(prompt, read_only)` owns the matching prompt and tool-mode
setup for isolated delegates and persistent teammates. Provider creation, task
purpose, stream factory, usage sinks, reservations, limits and history remain with
their existing owners. Main planning retains its attachment/instruction setup.
Advisor already uses the team hub; it must not grow another execution engine.

The audit fixed Unicode truncation at the code-review diff cap, propagation of
failed delegate reports as actual review errors, and stale delegate guidance that
omitted the native read/edit/write tools. No model call is made by a local preview.

## Deliberate boundaries

A read-only reviewer is never upgraded to a writing teammate. Opening a plan or
inspector cannot launch work. A branch changes context, not workspace files.
Synchronous one-shot delegates do not promise independent live interruption;
persistent teammates retain that capability and explicit history ownership.
No new token cap, auto-spawn, polling loop, transcript database or provider is
introduced by consolidation.

## Verification

Final workspace tests, strict Clippy, optimized release PTYs and offline performance
gate are recorded in `feature-consolidation-audit.md`.
