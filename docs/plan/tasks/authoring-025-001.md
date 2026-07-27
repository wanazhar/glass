---
id: authoring-025-001
scope: YAML workflow authoring compiler
status: completed
depends-on: []
---

# Objective

Add a YAML-first source boundary that compiles into the existing canonical
workflow runtime contract.

# Delivered

- Added bounded YAML and JSON source parsing with exact parser locations.
- Reused `WorkflowDefinition::from_value` and canonical serialization so
  authoring syntax does not introduce alternate runtime semantics.
- Added stable source hashes, deterministic YAML formatting, and structured
  compile diagnostics.
- Added a `serde_yaml` dependency and unit coverage for YAML/JSON equivalence,
  malformed-source reporting, and invalid-definition handling.
