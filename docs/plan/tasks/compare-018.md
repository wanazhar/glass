---
id: compare-018
scope: final competitive acceptance
status: blocked
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

## Evidence blocker

The reproducible harness and fail-closed evidence validators pass independent
code review, but the empirical gate has not run. Completion requires retained
100-iteration Glass, Playwright, and Playwright MCP reports plus revision-bound
ratified-metric, release-validation, and five-platform browser evidence. The
first external run was interrupted before producing output and was not used.
Until those artifacts exist, `best_in_class_eligible` remains false and Glass
must not use best-in-class language. Codex remains an explicit unsupported row
because this environment exposes no callable versioned black-box contract.

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
The Playwright MCP download scenario uses only its negotiated public tool
surface: `browser_click` triggers the authored download, and success requires
the server response to report a completed copy into its configured output
directory and the configured artifact to contain the expected fixture bytes.
The adapter must not open or race the server-owned temporary
Playwright artifact through unsafe runner code.
Eligibility fails closed without revision-bound evidence for every ratified
performance, protocol, release-validation, and real-browser platform gate.

Focused Linux aarch64 verification covers Node syntax and scorecard unit tests.
The interrupted one-iteration diagnostic produced no adapter report and is not
release evidence. This task remains in progress until the 100-iteration run and
all prerequisite evidence are published; Glass does not claim best in class.

Attempt 02 at reviewed revision `8101ca2` stopped the release sequence. A final
bounded trace localized Glass's failure to the missing CDP response for
popup-opening `mouseReleased`; the preceding press responds normally. The
Playwright baseline passed 11/11. Playwright MCP exposed a separate harness
race: its dialog callback started acceptance without awaiting completion, so
the next reset encountered the modal. The MCP adapter now awaits an explicit
dialog-handled promise. No Glass core workaround was accepted because the
existing task-local route invariant already holds and fire-and-forget input
would weaken correctness.

## Popup recovery review contract

The first causally verified popup implementation reached the real Chromium
missing-ack path, but adversarial review `5ff5d9a` blocked it: page code could
forge the witness, uniqueness and event-loss checks could race readiness,
cancellation could leak witness or attachment state, and MCP erased typed popup
failures. Those findings must pass independent re-review before another full
acceptance run.

The approved correction uses a 500 ms `click_expect_popup`-specific
`mouseReleased` acknowledgement window. Ordinary `click` and the global CDP
timeout do not change. Recovery uses an isolated-world, exact-backend-node
trusted-click witness that page code cannot call or monkeypatch; all witness and
temporary-attachment resources have cancellation-safe bounded cleanup. After
readiness, success requires a final authoritative target-set uniqueness,
liveness, and opener check plus a final topology sequence/loss-epoch check.
Every failure remains typed through MCP and no recovery step changes the active
route.

Focused popup measurements must report both the healthy release-ack latency
distribution and missing-ack recovery latency. Recovery is expected to complete
in under one second. One sample may diagnose the path but cannot support a
performance claim. The diagnostic run must remain popup-specific; the retained
100-iteration competitive acceptance gate stays blocked until review passes.
