---
id: compare-018
scope: final competitive acceptance
status: pending
depends-on: [release-017]
---

# Run the competitive acceptance gate

## Objective

Prove whether Glass earns its best-in-class positioning through published task
success, safety, efficiency, and usability evidence.

## Context

- `docs/plan/analysis/best-in-class-browser.md`
- `docs/architecture/automation.md`
- `benchmarks/README.md`

## Path

- `benchmarks/`
- `tests/`
- `README.md`
- `docs/`

## Requirements

- Run the versioned corpus against Glass, Playwright, and at least one mature
  agent-browser integration under controlled conditions.
- Compare Codex browser automation only through reproducible black-box tasks
  available to the test environment.
- Publish failures and unsupported scenarios, not only wins.
- Block best-in-class language on any wrong-target, safety, protocol, or
  resource-budget failure.
- Turn remaining non-blocking gaps into a prioritized follow-up backlog.

## Verification

- Independent rerun can reproduce report structure and fixture results.
- All correctness and safety gates pass.
- Resource and task-success claims link to raw reports and environment metadata.
- Full release validation and real-browser platform matrix pass.
