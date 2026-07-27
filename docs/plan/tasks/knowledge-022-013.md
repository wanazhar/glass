---
id: knowledge-022-013
scope: MCP knowledge-backed intent resolution
status: completed
depends-on: [knowledge-022-012]
---

# Objective

Expose explicit knowledge-backed intent resolution while preserving the
fresh-observation and guarded-action boundaries.

# Delivered

- Added `resolveIntentWithKnowledge` with profile and browser scope inputs.
- Required the persistent-profile policy capability before consulting the
  local store.
- Kept `resolveIntent` unchanged for callers that do not opt into knowledge.
- Used eligible target fingerprints only as bounded historical evidence on
  current candidates; stale, contradicted, and quarantined records are ignored.
