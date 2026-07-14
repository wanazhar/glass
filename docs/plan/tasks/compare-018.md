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

Required comparator adapters must complete every corpus row under the same
controls, but their task failures do not disqualify Glass. Eligibility requires
Glass itself to achieve zero wrong actions, 100% deterministic fixture success,
every safety/protocol/resource prerequisite, task success greater than or equal
to every completed comparator, and at least one declared efficiency win using
comparable resource scopes. Comparator errors remain scored failures and the
adapter continues through the corpus; transport loss or an incomplete matrix
still fails the required-adapter gate.

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

The released Playwright MCP 0.0.78 adapter has a bounded, iteration-scaled
process ceiling because its negotiated public JSON-RPC workflow is materially
slower than the direct adapters. The ceiling is two minutes of setup and
shutdown headroom plus 30 seconds per controlled iteration (52 minutes for the
ratified 100-iteration run). This budget exceeds the measured healthy runtime
without granting any individual request more time. Its per-request deadline,
corpus, iteration count,
viewport, profile semantics, and correctness classification remain identical;
all latency remains measured and compared. After every complete iteration it
atomically replaces a caller-provided checkpoint containing the exact Git
revision, fresh cryptographic invocation identity, start time, and controlled
configuration, the completed matrix rows, recomputed
partial summaries, and explicit progress. Temporary checkpoint files are
cleaned after publication. A timed-out runner retains and validates this
checkpoint as partial diagnostic evidence, but partial evidence can never
satisfy an adapter, control, correctness, or best-in-class gate. The complete
adapter report and its schema remain unchanged.

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

## Attempt 05 checkpoint

Reviewed revision `c334b446890473d63104dea04c90d0490f712ba3` passed all 1,100
Glass rows and all 1,100 Playwright 1.61.1 rows with zero wrong actions.
Playwright MCP 0.0.78 then failed the download scenario in each of its first
two completed iterations: the public click tool returned `isError` with an
empty error section. The revision-, configuration-, and invocation-bound
checkpoint retains 20 successes and two failures. The adapter was stopped once
the hard gate was irrecoverable, so no downstream ratified benchmark,
fuzz/build envelope, or platform run followed. Attempt 05 remains fail-closed
and is not best-in-class evidence.

## Attempt 06 checkpoint

Reviewed revision `f7705415407dfb4bc0630aa3cfb8b989657013f5` again passed all
1,100 Glass and Playwright rows. The exact comparable runner-RSS gate records
8,970,240 bytes for Glass and 196,788,224 bytes for Playwright, a strict Glass
win. Playwright MCP remained healthy and checkpointed 58/100 iterations
(638 rows) before the declared 1,200,000 ms ceiling: 580 successes, 58 repeated
download failures, and zero wrong actions. The incomplete required matrix
blocks acceptance. This establishes that the current ceiling cannot contain
the public MCP workflow at its measured throughput; it is not hang evidence.
No downstream evidence stage ran.

## Fresh-observation performance blocker

Three 50-iteration measurements on reviewed release candidate `9981414`
reported fresh compact-observation p95 between 8.95 and 9.11 ms, above the
ratified 5 ms budget. Cached observation remained between 0.020 and 0.034 ms
p95. The threshold is not relaxed.

The approved optimization may reuse the named observation isolated world only
for the exact selected target and frame. It must never reuse an execution
context across a route change. A context invalidated by navigation may trigger
one bounded recreation and retry; all other protocol failures remain errors.
The two page-state samples must continue to bracket the accessibility read, so
mutation-race detection and structured-first correctness do not weaken.
Measured release-mode improvement, focused stale-context/route-isolation tests,
the full test and lint suite, and independent review are required before the
optimization can enter a release candidate.

The first implementation cached the isolated-world context while preserving
the two state samples and full accessibility read. It improved one 50-iteration
fresh-observe p95 from 9.04 ms to 8.28 ms, but still missed the 5 ms gate and
introduced unresolved single-flight and concurrent-route cache complexity.
Adversarial review rejected it, and the implementation was reverted. The next
candidate must target the dominant 4.1–5.1 ms Chrome accessibility-tree call
through a correctness-preserving event-driven design; depth caps, unbracketed
parallel reads, and threshold relaxation remain disallowed.
