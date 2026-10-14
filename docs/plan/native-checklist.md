# Native execution checklist

## Contract

The existing `state` tool owns durable notes and a structured session checklist.
Checklist revisions are independent of note revisions. Mutations use stable task
IDs, atomic operation batches, and bounded request receipts. A failed durable
append rolls back the entire checklist, including receipts. Notes remain backward
compatible. Checklist snapshots survive resume, fork, rewind, and compaction.

Statuses: pending, in_progress, completed, blocked, abandoned. Phases group tasks;
blocking requires a reason. Completion is an agent report, not verified evidence.
Tasks may name an agent owner and explicitly link a workspace task. Children own
their own checklists and cannot silently mutate a parent's checklist.

## Interface

`state.todo.ops` accepts init, add, update, remove, clear and configure operations.
An omitted or empty ops list reads the full checklist. Writes return the changed
rows and a summary; expected revisions and request IDs are optional guards.
Tasks created by add/init receive IDs from the host. IDs never recycle on clear.
Automatic continuation is off by default, explicitly enabled in the checklist UI
or configure operation. It runs only after successful model completion with
pending/active tasks, with a visible notice and a bounded no-progress guard.
Blocked tasks, cancellation, provider failure and requests for user input do not
trigger continuation. Verification evidence remains separate from task status.

## UI

```
conversation and tool output
working status
Tasks 3/7 complete · 1 blocked · ◆ Restore the session
❯ composer
```

The summary replaces the divider while tasks exist and restores the divider when
hidden or cleared; it consumes no additional rows. Narrow terminals show counts.
`/todo` opens the native checklist, supports details and edits, and returns through
the existing palette stack. During a turn it provides a read-only snapshot. One
Markdown projection is shared by tool output, modal details and transcript replay.
UI updates occur only after committed changes, layout changes or explicit edits.

## Integration and delivery

Approved plans seed checklist items from Markdown checkboxes. Legacy todo extension
operations route to the native state when configured, with an explicit import path
for surviving transcript snapshots. Model guidance teaches batch progress updates
and read-back after context loss. Compaction carries concise actionable tasks;
closed tasks and request receipts remain in durable snapshots outside model context.

Root implements directly as requested by the user; one Luna medium reviewer follows.
Final verification covers transactions, retry receipts, resume/fork/compaction,
planning, legacy compatibility, ownership, busy browsing and PTYs down to 20x8.
