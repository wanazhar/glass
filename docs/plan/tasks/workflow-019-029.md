---
id: workflow-019-029
scope: sensitive workflow output redaction
status: completed
depends-on: [workflow-019-028]
---

# Objective

Prevent declared sensitive workflow outputs from crossing the normal result
surface as literal values.

# Delivered

Output declarations accept `sensitive`. Extraction still validates the
declared scalar type and records bounded source/revision evidence, but the
returned output uses `null` and `redacted: true` instead of retaining the
value. Non-sensitive outputs preserve the existing typed result shape.
