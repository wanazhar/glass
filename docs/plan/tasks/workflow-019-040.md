---
id: workflow-019-040
scope: idempotency declaration integrity
status: completed
depends-on: [workflow-019-039]
---

# Objective

Prevent ambiguous idempotency declarations from weakening duplicate-effect
review in a workflow definition.

# Delivered

Validation now rejects duplicate `idempotencyKey` values with a path-aware
error. The key remains a bounded workflow declaration and is not represented
as a distributed deduplication guarantee.
