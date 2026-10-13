# Coding workflow delivery

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../INDEX.md) for supported user behavior.

Status: functional v1 delivered and verified locally on 2026-10-10. All twenty items are implemented with performance and consistent presentation as delivery requirements. See [verification and limitations](reviews/workflow-next-20-001.md).

## Contracts

- Implementation requires independent review. Preserve existing work and unrelated live processes.
- No extra runtime dependency or service starts on ordinary startup. Disk-backed data is bounded and loaded on demand. No background model polling.
- UI uses the shared theme, modal input, Markdown and CLI Page renderer. Esc closes overlays; 20×8, 40×24 and 80×30 remain supported.
- Full outputs stay locally accessible; model context and inspector previews have explicit caps. Metadata is not evidence that a model read content.
- Mutating review operations verify the selected patch is still current, then require explicit confirmation. Project policies cannot relax user restrictions.
- Tokens, failures, reviewer activity and estimates are visible. No fabricated provider usage or private reasoning.

## Delivery matrix

| # | Feature | Gate |
|---|---|---|
| 1 | Interactive change review | Hunk comments/revert/review state; stale patch refusal |
| 2 | Full code intelligence | Definition/references/hover/symbols/rename/code actions; UTF-16 positions and preview-only edits |
| 3 | Debug mode | Persist hypotheses, evidence and instrumentation; no unsolicited model calls |
| 4 | Request inspector | Metadata timing/events/errors/export; no prompt or secret copies |
| 5 | Context changes | Fingerprints and source/message changes per request |
| 6 | Cache diagnostics | Prefix changes versus reported cache usage; explicit unknown values |
| 7 | Shell completions | Bash/zsh/fish; common command metadata |
| 8 | Shared tasks | Atomic claims/dependencies/blockers/acceptance |
| 9 | Typed handoffs | Validated findings/checks/changes/questions/artifacts |
| 10 | Coordination | Overlapping ownership warnings; integration preview |
| 11 | Review checkpoints | Explicit triggers and bounded reviewer runs |
| 12 | Role models | Executor/reviewer/research presets with budgets |
| 13 | Structured outputs | Native bounded JSON/diagnostic/test/failure views |
| 14 | Artifact shelf | Local references, bounded previews, no eager context injection |
| 15 | Plugin development | Scaffold/validate/events/latency/failures |
| 16 | Policy inspector | Global hard denials and decision explanations |
| 17 | Browser verification | Optional integration, screenshots/console/check artifacts |
| 18 | Session-to-skill | Selected steps, draft/review/save, never execute during capture |
| 19 | Session picker | Cross-project search and lazy previews/fork |
| 20 | Real-task regressions | Repeatable fixtures with token/latency/edit/PTY metrics |

## Shared UI

```text
/workspace → Tasks · Debug · Artifacts · Requests · Policies · Roles
┌ Tasks ─────────────────────────────────────┐
│ pending  T1  parser      → T2              │
│ claimed  T2  regression  owner: reviewer   │
│ Type to search · Enter details · Esc back │
└────────────────────────────────────────────┘
```

Actual verification and review evidence is recorded as each slice lands. A feature is not marked delivered by a placeholder menu or documentation alone.
