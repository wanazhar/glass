---
id: compare-018
scope: final competitive acceptance
status: in-progress
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

## Accepted execution contract

The release gate uses `glass-local-v1` with 100 iterations, one pinned system
Chromium executable, a warm profile, and identical fixture state. Raw JSON and
an environment manifest are retained for every runnable adapter. A comparison
surface that is unavailable on the host is reported as unsupported with the
missing invocation contract; it is never simulated.

Glass may claim best-in-class only if every hard gate in the report schema
passes. Playwright is the required general automation baseline. The required
agent-focused baseline must drive the corpus through a released MCP/browser
integration, not through Glass internals. Codex is an additional black-box row
only when its browser automation surface is callable by the harness.
