# Subagent extension

Delegate one to eight independent tasks through one tool call. Each task receives
a fresh session and state. Writing tasks use the five core tools; read-only tasks
use inspection tools. Parent conversation history is not copied automatically.

## Execution

The host runs up to four tasks concurrently and returns reports in request order.
A task failure or worker panic produces an error in that task's slot; siblings
continue. The extension declares serial batch execution so two batches do not
run concurrently through the same extension process.

The extension forwards `run_subagents` to the host. If no runner is installed,
the host returns an explicit error. Generated reports retain delegated provenance;
review their contents before treating them as instructions or verified evidence.

## Definitions and isolation

Named definitions live under `~/.config/glass-core/agents/`:

```markdown
---
name: security-review
description: Inspect a diff for injection and credential-handling defects.
model: YOUR_MODEL_ID
---

Report concrete failure cases with file locations and evidence.
```

The body supplies the persona; the host preserves the explicit assignment.
The optional model ID uses the active connection. A task with `worktree: true`
runs in a detached worktree at HEAD. Parent uncommitted edits are absent, and
changes are not merged back automatically.

For persistent workers, follow-ups, transcript inspection, and interruption,
use [Persistent teammates](../../docs/agents.md). Apply-back behavior is documented
under [Worktrees](../../docs/workflows.md#worktree-isolation).
