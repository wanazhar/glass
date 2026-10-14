# Session checklist

[Documentation](../INDEX.md) · [Tools](../tools.md) · [Workflows](../workflows.md)

The native checklist tracks execution progress inside a conversation. It is part
of `state`, alongside durable notes, and requires no extension. `/todo` opens the
checklist. During a running turn the modal is read-only; once idle, it supports
adding, renaming, starting, completing, blocking, assigning, linking and removing
tasks. Escape follows the normal modal and palette navigation stack.

## Status and ownership

| Status | Display | Meaning |
| --- | --- | --- |
| pending | ○ | Work not started |
| in_progress | ◆ | Work currently being done |
| completed | ✓ | Agent or user reports the task complete |
| blocked | ! | Waiting; a reason is required |
| abandoned | – | Deliberately excluded from the remaining work |

Several tasks can be active when work is parallel. The host does not automatically
start tasks or reopen completed work. An assigned agent is a reference to a
teammate; assigning it does not launch work or grant permission. Each teammate
owns its own checklist. The parent reads reports or transcripts and updates its
own checklist explicitly. Task details can open the assigned agent's transcript.

A workspace task link identifies coordination and evidence stored separately.
Checklist changes do not claim workspace tasks, alter acceptance criteria, or
assert that checks passed. The evidence action displays the recorded acceptance,
status and checks. Completion remains reported progress, not independent proof.

## Model operations

Read the complete current list and its revision:

```json
{"todo": {}}
```

Create a phased checklist in one call:

```json
{
  "todo": {
    "ops": [
      {"op": "init", "items": [
        {"title": "Implement the change", "phase": "Build"},
        {"title": "Run the relevant checks", "phase": "Verify"}
      ]}
    ]
  },
  "request_id": "create-plan"
}
```

The host returns stable IDs such as `T1`. Update related tasks together:

```json
{
  "todo": {
    "ops": [
      {"op": "update", "id": "T1", "status": "completed"},
      {"op": "update", "id": "T2", "status": "in_progress"}
    ]
  },
  "expected": "revision returned by the checklist read",
  "request_id": "begin-verification"
}
```

| Operation | Fields |
| --- | --- |
| init | `items`: replace tasks with a new list |
| add | `items`: append tasks |
| update | `id`, then any of `title`, `phase`, `status`, `blocker`, `owner`, `workspace_task` |
| remove | `id` |
| clear | Remove every task; IDs are not recycled |
| configure | `auto_continue`, `hidden` |

New items require `title`; `phase` defaults to `Tasks`. `owner` and
`workspace_task` are optional. For an update, an empty owner removes assignment
and workspace task `0` removes the link. Leaving blocked status clears its reason.
Unknown operations, fields, IDs and invalid transitions reject the whole batch.
Use one state namespace per call: notes and checklist operations are separate.

`expected` guards the checklist revision, independently of note revisions.
`request_id` identifies the complete batch. Repeating a retained request returns
its receipt; reusing it with different operations fails. The last 256 request IDs
are retained, oldest first. Reads do not mutate or normalize the list. Writes
return up to twelve changed rows, with byte-bounded text and omitted counts,
and a progress summary rather than repeating the full list. At most 32 removed
IDs appear in a write response; the full list is always available through a read.
Storage limits are 256 tasks and 128 KiB of task data, with 512-byte titles,
128-byte phases and agent references, and 1024-byte blocker reasons. A batch
contains at most 256 operations. These are storage bounds, not a task execution
or model token budget.

## Presentation and persistence

The composer summary replaces its divider while the checklist is visible. Small
terminals show compact counts, including `!N` for blockers and `–N` for abandoned
tasks; wider terminals also show the current task. The full modal
remains scrollable. Native tool cards, modal details and transcript replay use the
same Markdown rendering and status colors. Display changes do not change tasks.

Snapshots and receipts are written together in the session log. A failed append
rolls back the complete update. Resume and branch restore the checklist at that
point in history. Compaction retains the full snapshot outside model messages;
the model receives a compact projection of up to twelve open tasks. Closed tasks
and receipts are retrieved only when needed. No provider call is made to restore,
show, edit or redraw the checklist. Approved `/plan` checkboxes populate it before
execution starts; plans without checkboxes become one task.

## Continuation

Automatic continuation is off by default. Enable it in `/todo` or with an explicit
configure operation. After a successful answer, GC can continue actionable work
with a visible notice. It never continues from a provider error or cancellation,
or when all remaining tasks are blocked. It also waits when an owned job or
teammate is working, or the final response asks the user a question. Question
detection is conservative text matching, not a guarantee of intent recognition;
mark user-dependent tasks blocked explicitly.

At most two automatic continuations occur per user turn. An unchanged checklist
revision stops continuation earlier. Pending tasks remain visible; the user can
continue the task or disable continuation. Continuations make normal provider
requests and consume tokens. UI activity and local state changes do not.

## Legacy todo extension

The bundled `todo` extension is now a compatibility adapter. Its add/check/uncheck/
remove/list calls operate on this native checklist; it keeps no separate list.
Native state operations can be used without enabling the adapter. For old sessions,
`/todo` offers **Import legacy checklist**, which reads the latest textual todo
snapshot retained in that conversation. Existing tasks are replaced only after an
explicit selection. If no snapshot survives, GC reports that it cannot recover it.
