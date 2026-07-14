# Compare 018 MCP process-budget review

## Scope

Review commits `8ba2024` and `252dc1f`, plus corrective commit `798182e`, for
fail-closed process-deadline behavior and contract consistency.

## Finding and resolution

The first implementation accepted calculated deadlines above Node's maximum
timer delay. Node would reduce such a timer to one millisecond, contradicting
the published scaled budget for very large configured iteration counts.
Commit `798182e` rejects budgets above `2,147,483,647` milliseconds and tests
the exact accepted and rejected iteration boundary.

## Verdict

PASS. The ratified 100-iteration deadline is `3,120,000` milliseconds, the
30-second request deadline is unchanged, invalid and overflowing iteration
counts fail closed, and all 17 benchmark tests pass.
