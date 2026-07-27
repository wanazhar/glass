---
id: authoring-025-002
scope: workflow authoring diagnostics and CLI
status: completed
depends-on: [authoring-025-001]
---

# Objective

Provide deterministic static safety diagnostics and offline CLI operations for
workflow source.

# Delivered

- Added stable diagnostics for missing postconditions, unknown transaction
  behavior, unsafe retries, non-idempotent steps without markers, and
  unmarked sensitive inputs.
- Added `glass workflow validate`, `compile`, `format`, and `lint`.
- Kept authoring operations before browser startup and reused the canonical
  workflow validator.
- Added parser, analyzer, and CLI smoke coverage.
