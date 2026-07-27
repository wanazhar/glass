---
id: workflow-019-032
scope: effect-marker browser fixture
status: completed
depends-on: [workflow-019-031]
---

# Objective

Prove duplicate-dispatch protection through the real local browser session.

# Delivered

The opt-in browser fixture now runs a workflow whose target is intentionally
missing while its `beforeRetry` marker is already visible. The workflow
completes after one pre-dispatch attempt, retains no action execution ID, and
records the step as committed with observed effect evidence. This verifies the
marker path does not dispatch a second action.
