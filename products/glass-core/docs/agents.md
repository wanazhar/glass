# Delegation and persistent teammates

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Persistent teammates

Open `/agents` or press Alt+A for the roster, attributed interactions, and
read-only worker transcripts. Tab switches views; `?` lists actions. Assign work
through the main conversation. You can interrupt or archive a selected worker;
Esc restores the main conversation and draft. Nested rows identify their depth.
The WebUI Work and teammates panel receives revision events and can watch a
selected transcript live. Closing the panel disconnects its event stream.

The native `teammate` tool creates persistent delegates. A delegate can create
its own children and inspect or message those children. Read-only restrictions
are inherited; a read-only coordinator cannot create a writing child. The CLI
provides the same explicit controls:

```sh
"$GC_BIN" agent create review "Review src/parser.rs for EOF handling"
gc agent --session MAIN_ID list
gc agent --session MAIN_ID transcript review
```

Reviewers default to read-only; `--write` permits edits under the remaining
applicable policy. CLI workers created explicitly by the human do not trigger
unsolicited main-model turns. Model-created children return completion reports
to their owning parent.

## Completion, limits, and restart

After delegating, the model should finish its turn. The host delivers completion
reports and resumes the parent automatically, without model status polling.
Ready sibling reports are coalesced, and nested coordinators process their child
reports before the root resumes. Reports remain attributed output that the
parent must verify. The foreground CLI waits up to two minutes for pending work;
longer work remains available through the persistent session interface.

Each tree permits four active workers, 32 retained workers, and three child
levels. Automatic continuations allow at most 16 activations across the tree
and eight per worker. Delegated requests and automatic root follow-ups share a
200,000-token allowance, including reservations and conservative estimates when
usage is unavailable. Ordinary human-requested root turns retain their normal
session budget. Automatic root follow-ups also have a 24-round and 16,000-output-
token ceiling. The shared cost ledger remains authoritative.

A nonempty checklist with no pending or in-progress tasks stops automatic work.
Human interruption stops the tree. Hitting an allowance prevents further
requests; it does not grant models permission to reset the allowance. Inspect
or explicitly resume from another terminal:

```sh
gc agent --session MAIN_ID workflow inspect
gc agent --session MAIN_ID workflow stop
gc agent --session MAIN_ID workflow resume
# Explicitly change the tree token allowance when needed:
gc agent --session MAIN_ID workflow resume 300000
```

Resume resets activation allowances and retains token accounting and history.
Only the human can resume the workflow. Resume a parked worker separately with
an explicit assignment when needed. Restart restores saved records and parks
uncertain work rather than replaying tools or pending automatic activations.
Idle workers hold no provider request. Local viewers make no model requests;
transcripts load in bounded pages rather than filling the parent's prompt.

See the [persistent-agent contract](plan/persistent-agents.md) for the original
contract and [lifecycle audit](plan/execution-lifecycle-audit.md) for ownership.

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
