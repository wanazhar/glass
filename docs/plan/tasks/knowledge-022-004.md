---
id: knowledge-022-004
scope: explicit knowledge consultation surface
status: completed
depends-on: [knowledge-022-003]
---

# Objective

Expose a safe way to compare every stored record with fresh session dimensions
before knowledge-assisted observation or intent work is added.

# Delivered

- Added `KnowledgeStore::assess`, which returns bounded assessment evidence for
  every record without mutating the store.
- Kept fresh scope, landmark, and freshness checks as the source of truth;
  stored data cannot produce an executable browser reference.

Fresh-only operation remains the default for browser observations until a
later phase adds a measurable model-reuse path.
