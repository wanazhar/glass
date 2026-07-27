---
id: workflow-019-028
scope: workflow action execution correlation
status: completed
depends-on: [workflow-019-027]
---

# Objective

Preserve the action-level correlation IDs emitted by the shared executor in
workflow step results.

# Delivered

Batch success outcomes now carry their executor `executionId`, and workflow
steps retain the returned IDs in order under `executionIds`. This preserves
retry history without inventing an ID for operations that do not emit one.
