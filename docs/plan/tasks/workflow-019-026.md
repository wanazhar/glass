---
id: workflow-019-026
scope: independent workflow trace schema version
status: completed
depends-on: [workflow-019-025]
---

# Objective

Make workflow traces independently versioned so replay and migration can
evolve without coupling trace changes to workflow definitions or checkpoints.

# Delivered

- Trace exports include `schemaVersion: 1`.
- Replay rejects unsupported explicit trace versions before inspecting events.
- Traces produced by older local builds that omit `schemaVersion` remain
  readable as version 1 through a serde default.
