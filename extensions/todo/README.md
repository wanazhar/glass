# Task checklist extension

The optional `todo` extension provides a session-local checklist with add, check,
uncheck, remove, and list operations. Each result includes the current checklist.

## Behavior and limits

The list is held in the extension process and is lost when that process ends.
Use the workspace task board for durable tasks, dependencies, ownership, and evidence.

See the [user reference](../../docs/workflows.md#tasks-findings-and-artifacts) and the implementation in this directory.
