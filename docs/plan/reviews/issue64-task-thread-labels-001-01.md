# Independent review: issue64-task-thread-labels-001

## Scope reviewed

Reviewed task `53c11014`, both task-to-thread picker projection paths, the
focused projection test, and the TUI documentation. No source behavior was
changed and no broad tests were run during this review.

## Findings

- Task-to-agent association is matched by exact `TaskSnapshot.assigned_agent`
  and `AgentSnapshot.id` equality.
- Task-backed rows include role, task ID, and task title. Unmatched, manually
  spawned, or unassigned agents retain the role-only label.
- Existing ordering and viewport clipping remain unchanged; task lookup errors
  degrade to role-only labels.
- The initial wording said “current assignments,” although the scheduler may
  retain associations for terminal tasks. The helper comment and task record
  now describe recorded task-to-agent associations and the terminal case.

## Conclusion

**PASS after wording reconciliation.** The test is a focused projection-level
check; it does not claim an end-to-end scheduler-backed PTY workflow.
