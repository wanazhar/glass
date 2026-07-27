---
id: workflow-019-025
scope: post-dispatch resume-required status
status: completed
depends-on: [workflow-019-024]
---

# Objective

Prevent post-dispatch workflow failures from being presented as ordinary safe
failures.

# Delivered

Action failures and postcondition failures after dispatch now return the
explicit `resume_required` workflow status. The step record still preserves
`failedAfterDispatch`, dispatch acknowledgement, effect evidence, and the
current revision. Callers must reconcile browser state before retrying or
continuing; Glass does not automatically redispatch the step.
