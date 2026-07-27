---
id: knowledge-022-010
scope: knowledge assessment fixtures and scorecard
status: completed
depends-on: [knowledge-022-009]
---

# Objective

Make scope, freshness, contradiction, quarantine, and privacy boundaries
deterministic and repeatable without requiring a browser or external service.

# Delivered

- Added `benchmarks/scenarios/knowledge-v1.json` with eligible, stale,
  out-of-scope, contradicted, and quarantined cases.
- Added `knowledge_scorecard` to validate the corpus against the runtime
  assessment contract.
- Checked fixture payloads for bounded, non-sensitive knowledge data.
