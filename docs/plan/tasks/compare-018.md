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

## Implementation checkpoint

Commits `c3b3500` and its review-fix milestone add a versioned, fail-closed
acceptance runner. Playwright 1.61.1 and released `@playwright/mcp` 0.0.78 stay
in a temporary npm prefix and use one explicit Chromium path, a verified
1280x720 viewport, and equivalent fresh-ephemeral single-session profile
semantics. Codex remains explicitly unsupported because no callable harness
contract exists.

Reports retain bounded raw output and environment metadata. Commands and MCP
requests have deadlines, setup failures still publish an aggregate report, and
the runner validates every scenario/iteration pair and recomputes summaries.
Eligibility fails closed without revision-bound evidence for every ratified
performance, protocol, release-validation, and real-browser platform gate.

Focused Linux aarch64 verification covers Node syntax and scorecard unit tests.
The interrupted one-iteration diagnostic produced no adapter report and is not
release evidence. This task remains in progress until the 100-iteration run and
all prerequisite evidence are published; Glass does not claim best in class.
