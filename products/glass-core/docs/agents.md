# Delegation and persistent teammates

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Persistent teammates

Open `/agents` or press Alt+A for the roster, main/worker interactions, and worker
transcripts. Tab switches views; `?` lists actions. You can message, interrupt,
resume, or archive a selected worker. Narrow terminals display one view at a time.
Esc returns to the main conversation and restores the draft.

The main model accesses agent controls through CLI workflows exposed on demand:

```sh
"$GC_BIN" agent create review "Review src/parser.rs for EOF handling"
"$GC_BIN" agent wait review 10
"$GC_BIN" agent send review "Also inspect recovery.rs"
```

From another terminal, specify the owning session:

```sh
gc agent --session MAIN_ID list
gc agent --session MAIN_ID transcript review
```

`gc agent --help` lists model selection, limits, worktree isolation, transcript
paging, and lifecycle controls. Reviewers default to read-only; `--write` explicitly
permits writing under the remaining applicable policy. Workers cannot recursively
control the parent's agents.

## Context, limits, and restart

Each teammate has a stable ID, durable session, task notes, and compactable context.
Idle workers hold no provider request or worker thread. Two workers can run at
once per owner; the roster is bounded to 32 entries. Full transcripts load in
bounded pages and are not automatically copied into the parent's prompt.

The parent can choose a worker's model, task, context, and supported limits.
Receipts expose reported usage, estimates where needed, remaining allowance,
reservations, and recovery guidance. Output allowance includes reasoning. Review
limits in relation to the selected model before assigning a large task.

Interrupting retains history. Resume requires an explicit follow-up. Restart
restores saved roster records and parks uncertain work for review rather than
replaying possibly completed tools or undelivered messages automatically. The
shared parent cost ledger remains authoritative. Idle local inspection makes no
model request.

See the [persistent-agent contract](plan/persistent-agents.md) for exact bounds
and [lifecycle audit](plan/execution-lifecycle-audit.md) for setup ownership.

## One-shot delegation

The optional `subagent` extension accepts one to eight independent tasks. Up to
four run concurrently in a batch, and reports return in request order. Each task
gets a fresh session/state and either the applicable base prompt or a saved persona.
Writing tasks receive the native tools available to their configured model;
read-only tasks receive inspection tools. Delegated tasks have no interactive
question or approval channel.
Parent conversation history is not copied automatically.

```toml
extensions = ["python3 /path/to/glass-core/extensions/subagent/subagent.py"]
```

Named definitions use Markdown frontmatter under
`~/.config/glass-core/agents/`, with `name`, `description`, and an optional `model`.
The body supplies the persona while the assignment remains explicit. `worktree:
true` isolates work at the repository's HEAD; parent uncommitted edits are absent.
See [Worktrees](workflows.md#worktree-isolation) for apply-back behavior.

A failed task receives an error in its report slot; siblings continue. Reports
are framed as delegated output. This marks provenance without making generated
text trustworthy. Synchronous one-shot batches do not provide the same independent
live interruption and persistence controls as teammates.

## Model-assisted review

`/code-review` requests a read-only review of the current diff. `/advisor` uses
the persistent team hub for opt-in review checkpoints, with separate usage and
recorded dispositions. Configure a reviewer role before requesting automatic
milestone reviews. Opening local review records does not invoke a model.

The optional `/oracle` extension requests a second opinion from a configured
alternate model. It uses a read-only delegate and the session's metered provider
factory. Configure its model in `~/.config/glass-core/oracle.json` or through the
supported extension setting. See the extension source and `gc advisor --help`
for their distinct controls.
