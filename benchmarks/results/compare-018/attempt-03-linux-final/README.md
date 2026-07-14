# Attempt 03: authoritative Linux adapter run

- Revision: `5a687a31ab8d8fee22d83fd45c824814ebb494b1`
- Chromium: `/snap/bin/chromium`, `Chromium 150.0.7871.46 snap`
- Host: Linux arm64
- Iterations: 100 per scenario
- Playwright: `1.61.1` from a temporary npm prefix
- Playwright MCP: `0.0.78` from the same temporary prefix

Command:

```sh
CHROME_PATH=/snap/bin/chromium \
  GLASS_SCORECARD_ITERATIONS=100 \
  GLASS_ACCEPTANCE_COMMAND_TIMEOUT_MS=600000 \
  GLASS_ACCEPTANCE_ALLOW_FAILURE=1 \
  GLASS_ACCEPTANCE_OUTPUT_DIR=benchmarks/results/compare-018/attempt-03-linux-final \
  node benchmarks/run-acceptance.mjs
```

`GLASS_ACCEPTANCE_ALLOW_FAILURE=1` retained the complete fail-closed aggregate;
it did not waive or alter any gate.

## Adapter outcomes

| Adapter | Outcome | Exact result |
|---|---|---|
| Glass 0.1.0 | failed hard gate | 1,093/1,100 successes, zero wrong actions. Popup verification failed closed in iterations 44, 47, 50, 55, 70, 80, and 100 with `TopologyLagged: popup topology changed during final authoritative verification`. |
| Playwright 1.61.1 | passed | 1,100/1,100 successes and zero wrong actions. |
| Playwright MCP 0.0.78 | adapter failed | Produced no report. The reset after the dialog scenario was rejected because the `Continue?` confirm modal remained open. |
| Codex browser | unsupported | No callable, versioned black-box contract is available to this harness. |

The Glass popup failures are a product-path race: the earlier 50 ms quiet phase
can complete before readiness, followed by topology movement during the final
authoritative `Target.getTargets` call. The final check correctly fails closed,
but the operation is not stable enough for the 100-iteration gate.

The Playwright MCP failure is an adapter/tool-completion defect: despite the
adapter awaiting its dialog-handling code, the following public
`browser_evaluate` reset observes the modal still open. No MCP report was
synthesized from the partial run.

## Gate decision

Only the three raw Glass resource budgets passed:

- peak Glass runner RSS: 8,904,704 bytes;
- compact context: 12,260 bytes; and
- release binary: 5,578,952 bytes.

`required_adapters_ran`, `controlled_environment`, `zero_wrong_actions`, and
`deterministic_task_success` are false because the MCP adapter failed and Glass
had seven typed popup failures. Ratified metrics, release validation, and the
real-browser platform matrix remain absent and false. Therefore
`best_in_class_eligible` is false.

Per the stop rule, no optimized benchmark, fuzz regression, ratified evidence
envelope, or cross-platform run followed this failed adapter gate. Raw reports,
stderr, command metadata, environment data, and the aggregate decision are
retained in this directory without modifying earlier attempts.
