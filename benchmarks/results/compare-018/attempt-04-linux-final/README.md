# Attempt 04: reviewed Linux adapter run

- Revision: `25fe22963dafb4a89ee755c4b6b0c7f84a7eca54`
- Chromium: `/snap/bin/chromium`, `Chromium 150.0.7871.46 snap`
- Host: Linux arm64
- Iterations: 100 per scenario
- Playwright: `1.61.1`, temporary npm prefix
- Playwright MCP: `0.0.78`, same temporary prefix
- Per-command deadline: 600,000 ms

Command:

```sh
CHROME_PATH=/snap/bin/chromium \
  GLASS_SCORECARD_ITERATIONS=100 \
  GLASS_ACCEPTANCE_COMMAND_TIMEOUT_MS=600000 \
  GLASS_ACCEPTANCE_ALLOW_FAILURE=1 \
  GLASS_ACCEPTANCE_OUTPUT_DIR=benchmarks/results/compare-018/attempt-04-linux-final \
  node benchmarks/run-acceptance.mjs
```

The allow-failure switch retained the fail-closed aggregate and did not waive a
gate.

## Adapter outcomes

| Adapter | Outcome | Exact result |
|---|---|---|
| Glass 0.1.0 | passed | 1,100/1,100 successes, zero wrong actions, hard gate passed. |
| Playwright 1.61.1 | passed | 1,100/1,100 successes, zero wrong actions, hard gate passed. |
| Playwright MCP 0.0.78 | timed out | Exceeded the declared 600,000 ms adapter deadline and produced no JSON report or stderr. The process was terminated by the bounded runner. |
| Codex browser | unsupported | No callable, versioned black-box contract is available to this harness. |

The MCP adapter remained alive with empty stderr until the runner deadline. A
zero-byte raw report is retained rather than synthesizing partial scenarios.
This result establishes a throughput/deadline failure, not task correctness or
incorrectness for the unreported MCP scenarios.

## Gate decision

Only the direct Glass resource budgets passed:

- peak Glass runner RSS: 8,941,568 bytes;
- compact context: 12,260 bytes; and
- release binary: 5,578,952 bytes.

Glass's measured Chrome process tree peaked at 1,197,563,904 bytes. Playwright's
Node runner peaked at 197,509,120 bytes; its adapter does not expose Chrome-tree
RSS.

Because a required adapter did not complete, `required_adapters_ran`,
`controlled_environment`, `zero_wrong_actions`, and
`deterministic_task_success` remain false at the aggregate level.
Revision-bound ratified metrics, release validation, and platform evidence were
not supplied, so those gates also remain false and `best_in_class_eligible` is
false.

Per the stop rule, no optimized benchmark, popup benchmark, malformed-MCP
regression envelope, package validation envelope, or cross-platform run followed
the failed adapter deadline. Raw reports, empty failed-adapter output, command
metadata, environment data, and the aggregate are retained without overwriting
earlier attempts.
