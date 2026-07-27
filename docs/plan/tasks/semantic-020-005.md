---
id: semantic-020-005
scope: CLI semantic observation surface
status: completed
depends-on: [semantic-020-004]
---

# Objective

Expose the versioned semantic observation contract through the existing CLI
without changing the legacy compact observation output.

# Delivered

`glass observe --level summary|interactive|structured|detailed|raw` returns
the semantic contract. `--region` performs a revision-checked scoped
expansion and requires an explicit semantic level. Semantic options cannot be
combined with deep DOM, screenshots, or form-value overlays.
