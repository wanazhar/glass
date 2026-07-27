---
id: knowledge-022-001
scope: versioned persistent knowledge record and store contract
status: completed
depends-on: [intent-021]
---

# Objective

Define a bounded, scope-aware knowledge record before adding a persistent
backend or allowing remembered data to influence observation or resolution.

# Delivered

- Added versioned record kinds, scope dimensions, provenance, confidence
  lifecycle, invalidation rules, bounded opaque model data, and lifecycle
  history.
- Added canonical serialization, deterministic SHA-256 content hashes, strict
  payload limits, duplicate-ID snapshot validation, and sensitive-field-name
  rejection.
- Added the machine-readable contract at
  `docs/schema/knowledge-v1.schema.json`.

No knowledge is loaded into a browser session or used to authorize actions in
this phase.
