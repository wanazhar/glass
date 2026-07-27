---
id: semantic-020-004
scope: revision-aware semantic diffs and conservative continuity
status: completed
depends-on: [semantic-020-003]
---

# Objective

Compare semantic observations without crossing target/frame/URL boundaries
or silently treating a stale reference as actionable.

# Delivered

Added typed region and target changes, route/revision compatibility checks,
and `with_changes_from` for attaching a validated change set to the newer
observation. Unique role/name/input-type matches produce advisory continuity
mappings with medium confidence; they do not authorize an action or replace
the revision-scoped reference. Change vectors and continuity mappings are
bounded and serialized by the versioned schema.
