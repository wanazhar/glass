# Coding workflows

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Work

`/work` joins recorded tools, owned jobs, loops, attention items, tasks, and
teammates. `/dashboard`, `/hub`, and `/workspace` are aliases for this destination.
`/tasks` opens the task board. `/activity` filters recorded work; `/attention`
filters items requiring attention.

Items retain their session, project, process, or agent owner. Opening an inspector
does not start or stop the underlying process. A tool opens recorded output;
a job exposes output/input/stop; a teammate retains transcript and message controls.
These views use existing records without another event database.

## Plan before execution

`/plan` runs the bounded read-only inspector. Shell commands, mutations, extension
tools, and delegation are unavailable during planning. Review the proposal before
choosing execution, which starts a normal coding turn under the configured policy.

```sh
gc run --plan --json "Describe the changes needed for incremental indexing"
```

`/init [notes]` uses the same read-only mode to draft `AGENTS.md`. Review, save, or
regenerate the draft. Replacing an existing file requires confirmation.
`gc init --json` returns a draft; `gc init --write` creates the file.

## Review and Git

| Command | Purpose |
| --- | --- |
| `/diff` | Browse changed files and hunks |
| `/review` | Inspect task summary, commands, statuses, and checkpoint diff |
| `/turns` | Inspect recorded change intervals |
| `/git` | Review changes, stage files or hunks, and confirm a commit |
| `/code-review` | Request a model-assisted correctness review of the current diff |
| `/pr` | Inspect pull-request checks and reviews, with explicit publication actions |

Local inspection makes no model request. `/code-review` uses a fresh read-only
delegate and consumes provider quota. Failed delegates report an error instead of
being saved as successful findings. A completion message alone is not evidence
that builds or tests passed.

Staging and commits require their explicit actions. Rename and metadata changes
use whole-file staging. Cancellation can arrive after Git completes an operation;
check repository status before repeating it. PR publication remains explicit.

## Checkpoints, undo, and rewind

Tasks in a Git workspace save before/after checkpoints. `/undo` restores only the
selected task's changes, preserving unrelated edits and refusing files changed
since that task. Git's staging index is left unchanged.

`/checkpoint` saves a baseline. `/checkpoints` lists saved checkpoints. `/restore
NAME` restores the chosen baseline after saving a recovery checkpoint. Explicit
restore can replace subsequent edits to affected files.

Snapshots cover tracked files and nonignored new files in the repository. Ignored
build output and submodule contents are excluded. Objects use
`refs/glass-core/<session>/`; manifests use `checkpoints/<session>/` in the data
directory. Checkpoints support uncommitted edits and an unborn branch without
creating commits. They are unavailable outside Git; ordinary agent work still runs.

`/rewind` creates a conversation alternative and offers file restoration separately.
Keeping files is the default. Restoration checks stale edits and coordinated active
work, but cannot establish exclusive authorship against every external editor.
A conversation branch alone leaves files unchanged.

## Tasks, findings, and artifacts

The workspace board records tasks, dependencies, owners, blockers, findings,
acceptance evidence, handoffs, and artifact references. Claims and updates are
validated before persistence. Findings retain locations, severity, evidence, and
disposition. Handoffs retain changes, checks, questions, and linked artifacts.

Use `gc workspace --help` for task, finding, handoff, role, and artifact commands.
Artifact previews are bounded; registering a file does not inject it into model
context. Ownership conflicts are warnings to resolve explicitly.

## Worktree isolation

A delegated task can use an isolated Git worktree at a verified commit. Parent
uncommitted changes are absent unless transferred explicitly. Nothing is merged
back automatically.

```sh
gc worktree list --json
gc worktree apply WORKTREE_NAME
```

Apply stages the source worktree, creates a binary-safe diff against its HEAD,
and applies it to the destination using Git's three-way support. The destination
patch is staged. Conflicts remain visible for resolution; no commit is created.
Inspect the result before committing. `--remove` removes a checkout only after a
clean apply. Cross-repository application is refused. Other controls are listed in
`gc worktree --help`.

## Debugging and optional services

The workspace debug workflow stores hypotheses, evidence, and instrumentation.
Language services provide definitions, references, diagnostics, rename, and code
actions when configured. Supported edits carry target revisions and require review
before application. Optional AST transactions and DAP integration are loaded on
demand. See `gc structure --help` and the installed guidance catalogue.

Browser verification can collect screenshots and console errors from an explicitly
exposed browser connection. Optional companion and editor clients use the existing
runtime. These services do not start during ordinary terminal startup. See
[Integrations](integrations.md) and [Automation](automation.md).

## Reusable instructions

Capture selected session steps to draft a skill, then review and save it. Capture
does not execute the proposed instructions. `/skill-doctor` reports skill sizes
and modification dates. See [Extending](extending.md).

## Detailed references

- [Workspace commands](reference/workspace.md): tasks, role limits, findings,
  diagnostics, policies, browser checks, and artifacts.
- [Workbench commands](reference/workbench.md): quoted context, portable handoffs,
  job input, PR publication, advisor cadence, schedules, and optional clients.

## Execution checklist

Approved plans populate the native session checklist before execution. Use `/todo`
to inspect phases, progress, blockers, assigned agents and workspace evidence.
Checklist completion does not prove a check passed. See the
[checklist reference](reference/checklist.md) for state operations and continuation.
