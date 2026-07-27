---
id: workflow-019-035
scope: indeterminate post-dispatch classification
status: completed
depends-on: [workflow-019-034]
---

# Objective

Distinguish an acknowledged dispatch with no effect witness from a known
postcondition failure.

# Delivered

Batch action errors after dispatch now terminate the step as `indeterminate`
and return `resume_required`. Postcondition verification failures after a
successful/effect-observed action retain `failed_after_dispatch`. Neither path
is automatically retried.
