---
id: workflow-019-027
scope: trace replay prefix validation
status: completed
depends-on: [workflow-019-026]
---

# Objective

Ensure offline trace replay cannot accept a trace that silently starts in the
middle of a workflow.

# Delivered

Replay now requires the first retained event to reference the first declared
step. A trace that begins at a later step is rejected with a path-aware
validation error before any replayed state is returned.
