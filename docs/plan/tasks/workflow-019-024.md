---
id: workflow-019-024
scope: runtime workflow budget outcomes
status: completed
depends-on: [workflow-019-023]
---

# Objective

Enforce workflow execution budgets at runtime and make exhaustion explicit to
callers.

# Delivered

- `maxSteps` is checked before each logical step or repetition dispatch.
- `maxDurationMs` is tracked across preconditions, steps, verification, and
  output extraction; verification receives only the remaining budget.
- Exhaustion returns `budget_exhausted` instead of being reported as ordinary
  failure or success.
- The next step is never dispatched after a budget boundary is reached, and
  remaining steps are recorded as `skipped`.

The existing action-level timeouts remain responsible for classifying a single
browser operation; the workflow budget controls the enclosing run.
