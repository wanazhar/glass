# Review: compare-018

Reviewed commit `c3b3500` against `docs/plan/tasks/compare-018.md`,
`docs/plan/analysis/best-in-class-browser.md`,
`docs/architecture/automation.md`, and the benchmark adapter contract.

## Findings

1. **P1 — blocking — the controlled-environment gate accepts a viewport and
   profile configuration that was not actually shared.**

   The contract requires the same profile state and viewport
   (`docs/plan/analysis/best-in-class-browser.md:163` and
   `docs/plan/tasks/compare-018.md:48-50`). The Glass adapter starts a headless
   session without setting its viewport, then reports 1280x720 and substitutes
   the caller-provided `GLASS_SCORECARD_PROFILE` label even though the actual
   session is always incognito with the internal profile name `scorecard`
   (`examples/scorecard.rs:56-72`, `examples/scorecard.rs:124-130`). Playwright
   really creates a 1280x720 page and reports `ephemeral-incognito`
   (`benchmarks/adapters/playwright-scorecard.mjs:32-35`,
   `benchmarks/adapters/playwright-scorecard.mjs:85-92`), while Playwright MCP
   reports `ephemeral-isolated` (`benchmarks/adapters/playwright-mcp-scorecard.mjs:16-19`,
   `benchmarks/adapters/playwright-mcp-scorecard.mjs:63-64`). The runner's
   `controlGates` compares viewport claims but not profile metadata
   (`benchmarks/run-acceptance.mjs:109-115`). Consequently
   `controlled_environment` can pass despite different actual viewport and
   profile settings. Configure Glass's viewport explicitly, define equivalent
   profile lifecycle semantics, and gate the actual controls rather than
   labels.

2. **P1 — blocking — `best_in_class_eligible` does not represent every
   ratified hard gate.**

   The scorecard ratifies representative task success, fresh/cached observe
   latency, fast-action overhead, idle RSS, and malformed-MCP survival in
   addition to the three resource fields used here
   (`docs/plan/analysis/best-in-class-browser.md:19-29`). The task also requires
   full release validation and the real-browser platform matrix
   (`docs/plan/tasks/compare-018.md:39-44`). However, the eligibility boolean is
   derived only from adapter completion/control metadata, deterministic fixture
   summaries, Glass peak runner RSS, compact-context bytes, and binary size
   (`benchmarks/run-acceptance.mjs:47-60`). It can therefore become true while
   correctness, safety, latency, representative-success, and platform gates
   have never run. Either ingest revision-bound evidence for every ratified
   gate or keep eligibility unconditionally false until those prerequisites
   are present.

3. **P1 — blocking — child-process and MCP failure paths are unbounded and can
   prevent the acceptance report from ever being published.**

   The architecture requires explicit bounds on every channel and retained
   collection (`docs/architecture/automation.md`, "Non-negotiable resource
   rules"). `run()` has no deadline and buffers all stdout and stderr chunks in
   memory until a child exits (`benchmarks/run-acceptance.mjs:93-106`). MCP
   requests likewise have no deadline; if the server exits, emits malformed
   output, or never answers, entries in `pending` are never rejected and the
   adapter hangs forever (`benchmarks/adapters/playwright-mcp-scorecard.mjs:134-174`).
   In addition, a build or npm-install failure escapes the top-level `try`
   after cleanup, before `environment.json` or `acceptance.json` is written
   (`benchmarks/run-acceptance.mjs:21-46`, `benchmarks/run-acceptance.mjs:61-78`),
   contrary to the requirement to publish failures. Add bounded streaming/file
   capture, per-command and per-request deadlines, process-exit rejection, and
   a top-level failure report that is written on prerequisite failures.

4. **P2 — blocking — report validation trusts adapter summaries instead of
   validating or recomputing the evidence matrix.**

   The runner checks only top-level keys, schema version, corpus name,
   iteration count, and total array length
   (`benchmarks/run-acceptance.mjs:118-121`). It does not apply
   `benchmarks/report-schema.json`, verify tool identity/version, require every
   `(scenario, iteration)` pair exactly once, validate expected/status values,
   or recompute successes, wrong actions, and `hard_gate_passed`. The hard gates
   then directly trust the supplied summary
   (`benchmarks/run-acceptance.mjs:51-55`). A duplicated or internally
   inconsistent report can therefore pass. Validate the complete schema and
   matrix and derive gate values from raw scenarios.

## Confirmed behavior

- Glass, Playwright, and released Playwright MCP adapters invoke real APIs; the
  agent-focused row does not call Glass internals.
- Playwright dependencies are pinned and installed below a temporary prefix,
  which is removed in `finally`.
- Codex is honestly reported as unsupported with the missing invocation
  contract; it is not simulated.
- README language explicitly withholds a current best-in-class claim.
- `node --check` passed for `benchmarks/run-acceptance.mjs`,
  `benchmarks/adapters/playwright-scorecard.mjs`, and
  `benchmarks/adapters/playwright-mcp-scorecard.mjs`.

## Conclusion

**blocked**

The harness has the right adapter shape and disclosure posture, but its control
claims, hard-gate aggregation, and bounded failure behavior are not yet strong
enough to support reproducible competitive acceptance.
