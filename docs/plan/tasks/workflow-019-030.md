---
id: workflow-019-030
scope: declarative workflow effect markers
status: completed
depends-on: [workflow-019-029]
---

# Objective

Add a bounded declarative effect-marker field for duplicate-effect
reconciliation.

# Delivered

Workflow steps now accept and validate `beforeRetry` using the existing
verification predicate contract. It is serialized canonically and included in
the published workflow schema. The marker is checked only after a proven
pre-dispatch failure and can commit an already-visible success without a
second dispatch; marker lookup errors stop the run. It never authorizes an
automatic post-dispatch retry.
