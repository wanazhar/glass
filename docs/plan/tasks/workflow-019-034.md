---
id: workflow-019-034
scope: strict workflow unknown-field validation
status: completed
depends-on: [workflow-019-033]
---

# Objective

Align runtime workflow validation with the published schema’s strict field
policy.

# Delivered

Top-level and nested contract objects now reject unknown fields with bounded,
path-aware errors before deserialization. Predicate composition is checked
recursively, so unsupported predicate fields cannot be silently discarded.
