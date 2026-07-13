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

---

## Re-review: `03a4522`

### Resolution of prior findings

1. **Controlled viewport/profile — resolved.** Glass now applies and verifies a
   1280x720 device-metrics override (`examples/scorecard.rs:74-91`). All three
   adapters use the declared `fresh-ephemeral-single-session` semantics, and
   the control gate compares profile, viewport, corpus, iteration count,
   temperature, and Chrome version (`benchmarks/run-acceptance.mjs:195-201`).

2. **Incomplete hard-gate aggregation — partially resolved, still blocking
   below.** The aggregate now includes the six omitted ratified thresholds and
   release/platform prerequisites (`benchmarks/run-acceptance.mjs:64-82`).

3. **Unbounded execution/failure publication — partially resolved, still
   blocking below.** Command output is bounded, commands and MCP requests have
   deadlines, MCP exit rejects pending requests, and an ordinary setup failure
   writes both aggregate files (`benchmarks/run-acceptance.mjs:33-101`,
   `benchmarks/run-acceptance.mjs:117-155`,
   `benchmarks/adapters/playwright-mcp-scorecard.mjs:134-203`). A missing-Chrome
   smoke check confirmed `environment.json` and `acceptance.json` are emitted
   with every gate false.

4. **Trusted summaries/incomplete matrices — partially resolved, still
   blocking below.** Scenario identities and iteration pairs are unique and
   complete, classifications and summaries are recomputed, and expected tool
   versions are checked (`benchmarks/run-acceptance.mjs:157-193`). Full report
   validation is still absent.

### Remaining findings

1. **P1 — blocking — malformed ratified-gate evidence crashes before the
   aggregate report is written.**

   `retainEvidence` accepts any revision-matching JSON object and marks it
   completed without validating `metrics` (`benchmarks/run-acceptance.mjs:204-215`).
   `prerequisiteGates` then reads `.metrics` outside any error boundary and
   immediately dereferences it (`benchmarks/run-acceptance.mjs:218-227`). A
   revision-matching file with no `metrics`, `metrics: null`, or an unreadable
   copied report throws a `TypeError` or parse error before
   `environment.json`/`acceptance.json` are written at lines 91-100. This
   contradicts the documented fail-closed behavior and the task requirement to
   publish failures. Validate and derive every prerequisite inside the guarded
   ingestion path; malformed evidence must produce `status: invalid` and false
   gates, not terminate the runner.

2. **P1 — blocking — release-validation and platform-matrix gates trust an
   unauditable boolean assertion.**

   The task requires the full release validation and real-browser platform
   matrix to pass and links claims to raw evidence
   (`docs/plan/tasks/compare-018.md:39-44`). The new ingestion accepts any JSON
   containing the current revision, `schema_version: 1`, and `passed: true`
   (`benchmarks/run-acceptance.mjs:204-229`); the repository provides no schema,
   producer, required platform/check matrix, or result validation for those
   files. A hand-authored three-field JSON file therefore sets each hard gate
   true without proving that one check ran. Define versioned evidence schemas
   and validate the required release checks/platform rows and their raw result
   references before deriving these gates.

3. **P2 — blocking — adapter reports still are not validated against the
   published report schema and malformed resource data can abort aggregation.**

   The adapter contract requires reports matching
   `benchmarks/report-schema.json` (`benchmarks/adapters/README.md:3-5`). The
   replacement validator checks exact keys for only the top-level and selected
   objects; it does not validate nested `resources.runner`/`resources.chrome`
   structures or most schema types and bounds
   (`benchmarks/run-acceptance.mjs:157-169`). For example, a report with
   `resources.runner: null` passes `validateReport`, enters the completed-report
   map, and then throws while evaluating the Glass budget at line 78, again
   preventing aggregate publication. Apply the full schema or equivalent
   exhaustive validation inside `runAdapter`; invalid reports must remain
   adapter failures and must never reach gate aggregation.

### Re-review checks

- `node --check` passed for the runner and both JavaScript adapters.
- A quick missing-`CHROME_PATH` run completed without installing dependencies
  and retained `environment.json` plus `acceptance.json` with all gates false.
- No 100-iteration run, dependency installation, or release evidence was
  executed as part of this review.

### Re-review conclusion

**blocked**

The normal-path controls and matrix recomputation are materially stronger, but
malformed evidence can still suppress the aggregate report, and the two release
prerequisite gates remain assertions rather than reproducible evidence.

---

## Final bounded re-review: `af3bd10`

This review is limited to the three code blockers recorded for `03a4522`.

### Code finding resolution

1. **Malformed ratified metrics — resolved.** Evidence is now validated inside
   `retainEvidence`'s guarded path before it is copied or exposed to gate
   derivation (`benchmarks/run-acceptance.mjs:214-227`). Missing, null, or
   incomplete metrics fail `exactKeys`, become `status: invalid`, and
   `prerequisiteGates` reads only optional validated `derived.metrics`
   (`benchmarks/run-acceptance.mjs:229-241`). A focused probe with missing
   metrics confirmed that both aggregate files were retained, the evidence was
   marked invalid, and all affected gates were false.

2. **Unauditable release/platform assertions — resolved.** The contract now
   declares the exact nine release checks and five platform targets
   (`benchmarks/acceptance-v1.json:9-17`). Evidence must identify its type,
   tested revision, producer name/version/command/run URL, and per-row raw
   report; release and platform rows are exact-key validated, must all pass,
   must cover the exact configured set once, and reject duplicates or extras
   (`benchmarks/run-acceptance.mjs:244-285`). The published versioned evidence
   schema documents the same structures
   (`benchmarks/prerequisite-evidence-schema.json`). Focused empty-matrix probes
   were reported invalid and could not set either hard gate.

3. **Nested resource validation — resolved.** `validateReport` now requires the
   exact runner and Chrome resource structures and validates PID, nullable RSS,
   size/count, scope, and startup bounds before inserting a report into the
   completed-report map (`benchmarks/run-acceptance.mjs:157-176`). Gate reads
   are additionally null-safe (`benchmarks/run-acceptance.mjs:78-80`). A null
   or malformed nested resource therefore remains an adapter failure rather
   than reaching aggregation.

### Final bounded checks

- `node --check` passed for `benchmarks/run-acceptance.mjs`,
  `benchmarks/adapters/playwright-scorecard.mjs`, and
  `benchmarks/adapters/playwright-mcp-scorecard.mjs`.
- Focused malformed ratified, release-check, and platform-matrix fixtures all
  produced `status: invalid`, false gates, `best_in_class_eligible: false`, and
  retained `environment.json` plus `acceptance.json`.
- No dependency installation, browser run, 100-iteration corpus, release
  validation, or platform matrix was executed in this bounded re-review.

### Code verdict

**pass**

No blocking code finding remains from review `19241f7`.

### Delivery evidence status

**blocked pending required evidence**

This code verdict does not satisfy the task's empirical release gate. The
100-iteration comparative reports, ratified-gate metrics and raw references,
full release-validation evidence, and real-browser platform-matrix evidence
remain absent from this review. Glass remains ineligible for best-in-class
language until those long-run artifacts exist and the runner accepts them.

---

## Focused measured-harness re-review: `6b18deb`

This review is limited to the failures retained from the first measured run.

### Findings

No blocking finding remains in the reviewed scope.

### Verified fixes

- Viewport validation now compares the numeric `width` and `height` fields
  structurally in both adapter validation and the cross-adapter control gate;
  JSON key insertion order can no longer reject Glass's valid viewport
  (`benchmarks/run-acceptance.mjs:176-179`,
  `benchmarks/run-acceptance.mjs:205-211`,
  `benchmarks/run-acceptance.mjs:296`).
- The Playwright adapter reads the HTML `<output>` element through its `value`
  property rather than calling the input-only `inputValue()` API
  (`benchmarks/adapters/playwright-scorecard.mjs:139-141`).
- Playwright MCP client creation now calls a hoisted factory that constructs an
  inline class, removing the top-level class temporal-dead-zone failure
  (`benchmarks/adapters/playwright-mcp-scorecard.mjs:16-19`,
  `benchmarks/adapters/playwright-mcp-scorecard.mjs:134-205`).
- Glass delayed content uses the public typed wait API before reading the
  expected state (`examples/scorecard.rs:232-246`). Popup and frame scenarios
  use public target/frame listing and selection APIs, return to the original
  page/main frame, and let the fixture's next reset close the retained popup
  (`examples/scorecard.rs:261-289`, `examples/scorecard.rs:334-350`,
  `tests/fixtures/scorecard.html:47-54`). Dialog and download scenarios exercise
  the public `accept_dialog` and `wait_for_download` APIs; download cleanup runs
  on both success and error paths (`examples/scorecard.rs:291-317`). These are
  current `BrowserSession` APIs also used by CLI/MCP dispatch, not simulated
  success or raw-CDP instrumentation.
- Commit inspection found no temporary tracing, debug output, fixture mutation,
  or benchmark-result manipulation.

### Checks

- `node --check` passed for the acceptance runner, Playwright adapter, and
  Playwright MCP adapter.
- `cargo test --example scorecard --locked` passed all 5 unit tests.
- No browser execution, dependency installation, or long acceptance run was
  performed in this focused review.

### Code verdict

**pass**

### Full-rerun decision

**Safe to rerun the full acceptance harness.** The known first-run structural,
Playwright API, MCP initialization, delayed-wait, and unsupported-Glass-scenario
failures are addressed through real current surfaces. This is a code-readiness
decision, not evidence that the 100-iteration gate will pass; the retained full
rerun remains authoritative for timing, cleanup, task success, and resource
outcomes.
