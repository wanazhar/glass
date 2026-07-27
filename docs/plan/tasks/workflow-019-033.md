---
id: workflow-019-033
scope: workflow type-field compatibility alias
status: completed
depends-on: [workflow-019-032]
---

# Objective

Keep the workflow contract compatible with definitions using the issue’s
`type` spelling while preserving one canonical serialized form.

# Delivered

Workflow input and output declarations accept `type` and `valueType` on input.
Glass emits `valueType` from canonical serialization, and the published schema
documents both forms as mutually exclusive alternatives.
