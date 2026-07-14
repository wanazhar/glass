# Attempt 06: full-matrix Linux timeout

- Revision: `f7705415407dfb4bc0630aa3cfb8b989657013f5`
- Chromium: `/snap/bin/chromium`, `Chromium 150.0.7871.46 snap`
- Host: Linux arm64
- Requested iterations: 100 per scenario
- Playwright: `1.61.1`
- Playwright MCP: `0.0.78`
- Common MCP request deadline: 30,000 ms
- Playwright MCP process ceiling and exact runtime: 1,200,000 ms

Glass completed 1,100/1,100 rows and Playwright completed 1,100/1,100,
both with zero wrong actions. Glass used 8,970,240 bytes peak runner RSS versus
Playwright's 196,788,224 bytes under the exact compatible
`primary-non-browser-runner-process-rss-v1` scope, so the declared efficiency
comparison passed.

Playwright MCP remained transport-healthy but reached its immutable 20-minute
process ceiling after 58 complete iterations: 638/1,100 rows, 580 successes,
58 failures, and zero wrong actions. Every failure was the public download
tool response previously observed; the adapter continued after each scenario
failure as required. The checkpoint is bound to run ID
`0acda2fb-a216-406e-be6f-82b468885c7e`, invocation start
`2026-07-14T07:18:33.907Z`, and this exact revision/configuration.

The measured rate (roughly 20.7 seconds per iteration over the declared total
runtime) proves a harness-budget mismatch rather than a hang. Because the MCP
matrix is incomplete, `required_adapters_complete_exact_matrix`, controlled
comparison, and not-trailing gates fail closed. Best-in-class eligibility is
false. No ratified benchmark, fuzz/build, or cross-platform stage followed.

This directory retains complete Glass/Playwright reports, the validated MCP
partial checkpoint, zero-byte MCP final output, all stdout/stderr streams,
environment and command metadata, and the aggregate gate decision.
