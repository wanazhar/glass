# Attempt 05: reviewed Linux acceptance stop

- Revision: `c334b446890473d63104dea04c90d0490f712ba3`
- Chromium: `/snap/bin/chromium`, `Chromium 150.0.7871.46 snap`
- Host: Linux arm64
- Requested iterations: 100 per scenario
- Playwright: `1.61.1`
- Playwright MCP: `0.0.78`
- Common MCP request deadline: 30,000 ms
- Playwright MCP process ceiling: 1,200,000 ms

Command:

```sh
CHROME_PATH=/snap/bin/chromium \
  GLASS_SCORECARD_ITERATIONS=100 \
  GLASS_ACCEPTANCE_COMMAND_TIMEOUT_MS=600000 \
  GLASS_ACCEPTANCE_ALLOW_FAILURE=1 \
  GLASS_ACCEPTANCE_OUTPUT_DIR=benchmarks/results/compare-018/attempt-05-linux-reviewed \
  node benchmarks/run-acceptance.mjs
```

The allow-failure switch retained the fail-closed aggregate; it did not waive
any gate.

## Results and stop decision

Glass completed 1,100/1,100 scenarios and Playwright completed 1,100/1,100,
both with zero wrong actions. Playwright MCP completed two checkpointed
iterations (22 rows): 20 succeeded and the download scenario failed in both
iterations. Its public `browser_click` response was marked as an MCP error with
an empty `### Error` section after showing the authored click. No result was
synthesized from the download side effect.

The checkpoint is bound to run ID
`ec8f0f63-2a3b-4cb9-ae70-7c12283f38e0` and invocation start
`2026-07-14T07:03:58.682Z`. It is retained as
`raw/playwright-mcp.checkpoint.failure.json`. Once the second identical failure
made the hard gate impossible, the adapter was terminated per the stop rule so
the runner could publish its aggregate and environment. The aggregate records
that deliberate termination as an adapter failure; the preserved checkpoint
contains the scenario-level defect and progress.

`required_adapters_ran`, controlled-environment, deterministic-success, and
zero-wrong-action aggregate gates therefore remain false, as does
`best_in_class_eligible`. No ratified benchmark, fuzz/build evidence, or
cross-platform stage followed.

The directory retains both complete baseline reports, the partial MCP
checkpoint, zero-byte MCP final report and stderr, runner stdout/stderr,
environment metadata, command deadlines, and the fail-closed aggregate.
