---
id: integration-031-001
scope: Four-pillar release conformance and demonstration
status: pending
depends-on: [surface-031-002, backend-031-002, presentation-031-002, workspace-031-002, memory-031-002, experience-031-001]
---

# Objective

Prove the four pillars and Experience Layer operate as one product through real integrated scenarios, conformance fixtures, security/privacy checks, performance measurements, migration documentation, and release-gate evidence. This task must not paper over missing pillars with mocks.

# Context

- `docs/plan/analysis/release-031.md`
- Issue #31 integrated demonstration and six release gates
- all prerequisite task docs and current `docs/INDEX.md`

# Path

`tests/`, fixtures, benchmarks, security/recovery documentation, migration notes, release validation scripts, and final evidence. Keep Windows/native binary claims out of scope unless separately certified.

# Verification

Run the complete local conformance suite: verified memory revisit, non-DOM surface and semantic bridge, non-CDP task where capabilities permit, opaque fail-closed task, protocol shock, human takeover/reconciliation, shared workspace references, replay/diff, semantic fallback, and bounded performance/latency metrics. The final report must state any unmet issue gate; do not publish or tag.
