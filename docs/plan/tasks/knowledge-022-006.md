---
id: knowledge-022-006
scope: fresh page-family record construction
status: completed
depends-on: [knowledge-022-005]
---

# Objective

Create bounded page-family knowledge from fresh semantic structure without
persisting current browser handles or page contents.

# Delivered

- Added `KnowledgeRecord::from_page_observation` with explicit build options.
- Records retain only page kind, region kinds, provenance, scope, and
  invalidation landmarks.
- Current target references, labels, text, accessibility trees, and form
  values are excluded and covered by a fixture-backed test.

Records start in `observed` state; promotion to `verified` still requires a
separate fresh verification transition.
