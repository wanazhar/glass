# Re-review: quality-007 browser automation scorecard

Reviewed branch through `3f5ef63` (`fix: make browser scorecard outcomes honest`).

Conclusion: **blocked**. The fix resolves the prior P1 scenario-honesty and
scenario-ID classification defects, strengthens the schema substantially, and
accurately documents post-startup RSS and runner scope. Two report-contract
issues remain before the task can be considered complete.

## Resolved findings from review 01

- Glass no longer simulates popup, frame, dialog, or download success through
  JavaScript; these produce explicit `unsupported` outcomes
  (`examples/scorecard.rs:226-235`). The fixture and Playwright adapter now
  exercise real popup, frame, dialog, download, delayed-content, and pointer
  reflow behavior (`tests/fixtures/scorecard.html:25-48`,
  `benchmarks/adapters/playwright-scorecard.mjs:165-208`).
- Wrong-action classification is driven by each scenario's declarative
  forbidden outcomes rather than its id (`benchmarks/scenarios/v1.json:6-16`,
  `examples/scorecard.rs:150-163`). The forced self-test now keeps the intended
  locator unchanged and injects the faulty target result
  (`examples/scorecard.rs:168-180`).
- The misleading cold implementation was removed from both adapters, and
  schema v1 constrains temperature to `warm`
  (`examples/scorecard.rs:112-118`,
  `benchmarks/adapters/playwright-scorecard.mjs:82-89`,
  `benchmarks/report-schema.json:20-27`).
- The report schema now defines nested structure, types, nullability, enums,
  ranges, and additional-property policy
  (`benchmarks/report-schema.json:1-126`).
- Resource documentation now explicitly calls the samples post-startup
  workflow peaks (`benchmarks/README.md:27-30`,
  `docs/plan/analysis/best-in-class-browser.md:46-49`).
- The adapter contract permits and requires disclosure of single-process
  runner measurement, matching the Playwright report
  (`benchmarks/adapters/README.md:8-11`,
  `benchmarks/adapters/playwright-scorecard.mjs:97-108`).

## Remaining findings

### P2 — blocking: task metadata contract still requires cold runs

Removing the inaccurate cold mode fixes the behavior reviewed previously, but
the task remains `status: done` while its requirement still says the harness
records `warm/cold` metadata (`docs/plan/tasks/quality-007.md:1-5`, `37-40`).
Schema v1 now expressly forbids a cold value
(`benchmarks/report-schema.json:26`), and the adapter contract defers cold
semantics to a future corpus (`benchmarks/adapters/README.md:5-8`). Thus the
implementation and its own completion claim do not satisfy the ratified task.
The task requirement should be explicitly revised to warm single-session
metadata with cold lifecycle measurement deferred to a named task, or
equivalent cold semantics must be implemented before `quality-007` remains
done.

### P2 — blocking: `summary.failures` overlaps every non-success outcome

Both adapters compute `failures` as total outcomes minus successes
(`examples/scorecard.rs:105-108`,
`benchmarks/adapters/playwright-scorecard.mjs:73-80`). Consequently the recorded
baseline's 33 outcomes produce `failures: 18` alongside `wrong_actions: 3` and
`unsupported: 12`, even though the analysis describes three *ordinary*
failures (`docs/plan/analysis/best-in-class-browser.md:38-44`). This contradicts
the task completion statement that the report separates ordinary failures and
wrong actions (`docs/plan/tasks/quality-007.md:54-56`) and makes the three
summary counters non-partitioning, with no schema description of the overlap.

Machine consumers cannot tell from the field name whether `failures` means
ordinary `status == "failure"` or all non-successes. Count ordinary failures
directly (leaving `hard_gate_passed` based on all non-success outcomes), or
rename the aggregate to an explicit `non_successes` and add a separate ordinary
failure count. The schema/documentation should state and test the invariant.

### P3 — non-blocking: declarative wrong-action oracles are not exhaustive

The generic classifier is fixed, but most scenarios have an empty forbidden
list and the form scenario only forbids `submitted:`
(`benchmarks/scenarios/v1.json:9-16`). An implementation that activates a
different fixture control during those scenarios is generally recorded as an
ordinary failure, not `wrong_action`. The hard gate still fails, so this does
not invalidate pass/fail gating, but `summary.wrong_actions` is not yet a full
count of wrong target side effects. Future corpus versions should record target
identity/side effects directly or enumerate all fixture wrong-action outcomes.

## Focused verification

I reused the branch's recorded full validation. Independent inexpensive checks
passed:

- `cargo test --example scorecard` — 4 passed;
- `node --check benchmarks/adapters/playwright-scorecard.mjs`;
- `git diff --check 3837b7c 3f5ef63`.

No implementation changes were made during this re-review.
