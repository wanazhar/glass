---
id: authoring-025-005
scope: workflow preview diff and fixtures
status: completed
depends-on: [authoring-025-004]
---

# Objective

Provide offline review evidence for workflow shape and migrations without
starting Chrome or printing action values.

# Delivered

- Added deterministic `workflow preview` output with action shape, effect and
  retry metadata, postcondition coverage, and input names.
- Added deterministic `workflow diff` output with canonical hashes, stable
  step changes, risk levels, and migration guidance.
- Added a checked-in YAML authoring fixture and an integration gate covering
  compilation, inferred sensitivity, preview redaction, and diagnostics.
- Updated README, CLI, workflow, changelog, and authoring documentation.
