---
id: knowledge-022-005
scope: fresh semantic observation with explicit knowledge assessment
status: completed
depends-on: [knowledge-022-004]
---

# Objective

Expose knowledge consultation through the session API without allowing stored
records to replace fresh browser state or authorize actions.

# Delivered

- Added `BrowserSession::semantic_observe_with_knowledge`.
- Added explicit `freshOnly` and `assessed` report modes.
- Kept the browser observation fresh in both modes; assessed reports include
  eligible, stale, and out-of-scope record IDs plus bounded explanations.
- Kept the store read-only and excluded current target references from the
  knowledge report.

This phase reports eligible knowledge but does not claim model-reuse savings;
the measurable reuse path remains a later phase.
