# Review: quality-007 browser automation scorecard

Reviewed commit: `3837b7c` (`test: establish browser automation scorecard`)

Conclusion: **blocked**. The implementation builds and its focused unit tests
pass, but the corpus does not yet measure several capabilities named by the
task, and the generic outcome classifier cannot enforce the zero-wrong-action
contract across the corpus. Those are scorecard-validity issues rather than
future feature failures.

## Findings

### P1 — blocking: named scenarios bypass the capabilities they claim to score

The corpus lists frame, dialog, download, popup, reflow, and delayed-content
workflows as coverage (`benchmarks/scenarios/v1.json:8-13`), but several Glass
drivers do not exercise those workflows:

- The frame case executes the iframe click and writes the parent result through
  JavaScript, then returns a constant (`examples/scorecard.rs:206-209`). It
  cannot distinguish frame automation support from arbitrary evaluation.
- The fixture's dialog button never opens a dialog; it only writes
  `dialog-requested` (`tests/fixtures/scorecard.html:41`), so
  `examples/scorecard.rs:210-213` measures an ordinary click.
- The download case accepts the anchor's synchronous `onclick` side effect
  (`tests/fixtures/scorecard.html:42`, `examples/scorecard.rs:214-217`) without
  observing that a download started or completed.
- The popup case reads a parent-page value and closes the popup via evaluation
  (`examples/scorecard.rs:200-204`); it never proves that Glass discovered or
  controlled the new target.
- Reflow is completed before target resolution begins
  (`examples/scorecard.rs:177-182`), so it does not test movement between
  resolution, hit testing, and dispatch.
- Delayed content is read once with no wait (`examples/scorecard.rs:184-190`).
  Since its timer starts at initial fixture load and the scenario runs fourth,
  ordinary preceding work can make it pass without any waiting behavior.

The Playwright adapter does exercise popup, frame, download, and wait APIs
(`benchmarks/adapters/playwright-scorecard.mjs:167-200`), which also means the
two adapters are not running equivalent workflows. This conflicts with the
task requirement for deterministic workflows covering these areas and with
the architecture's identical-workflow comparison contract
(`docs/architecture/automation.md`, integration item 12). Passing results would
overstate Glass capability and make cross-tool task-success rates invalid.

### P1 — blocking: wrong actions are recognized only in one hard-coded scenario

Both classifiers label an unexpected actionable result `wrong_action` only
when the scenario id is `duplicate-label`; all other unexpected results become
ordinary failures (`examples/scorecard.rs:78-85`,
`benchmarks/adapters/playwright-scorecard.mjs:49-54`). A wrong overlay, reflow,
form, popup, or future scenario action therefore does not increment
`summary.wrong_actions`. The corpus has no declarative oracle for forbidden
side effects or selected target identity, and the unit test merely counts a
preconstructed status (`examples/scorecard.rs:418-428`).

The forced mode changes the requested locator to the explicitly wrong label
(`examples/scorecard.rs:156-163`) rather than injecting a deliberately wrong
target implementation, so it does not demonstrate generic detection either.
This fails the verification requirement that fixture outcomes detect a wrong
target implementation and prevents the zero-wrong-action gate from being a
corpus-wide invariant.

### P2 — blocking: `cold` reports do not have stable or comparable semantics

Glass records `temperature: "cold"` but only renavigates the same live browser
between iterations, after the preceding iteration (`examples/scorecard.rs:98-100`).
The first iteration follows the same startup path as `warm`, and browser,
context, caches, and profile remain alive. The Playwright adapter ignores
`GLASS_SCORECARD_TEMPERATURE` and always emits `warm`
(`benchmarks/adapters/playwright-scorecard.mjs:82-88`). This contradicts the
adapter contract requiring common warm/cold metadata
(`benchmarks/adapters/README.md:3-6`) and makes the public `cold` option in
`benchmarks/README.md:22-24` misleading. Either the mode needs a precise,
implemented lifecycle definition across adapters or it should not be emitted.

### P2 — blocking: the published schema does not stabilize report semantics

`benchmarks/report-schema.json:8-18` mostly specifies required property names
without property types, enums, numeric ranges, nested required fields, or
`additionalProperties` constraints. For example, it accepts strings for
counts, negative latency, arbitrary status values, missing outcome fields,
and structurally empty `tool`, `run`, `environment`, `resources`, and `summary`
objects beyond their few top-level requirements. The schema also does not
state which metrics are nullable even though the Playwright adapter relies on
explicit nullability (`benchmarks/adapters/playwright-scorecard.mjs:93-114`).
Consequently materially incompatible reports can validate as schema version 1,
which does not satisfy the task's stable machine-readable report requirement.

### P2 — non-blocking: peak resource sampling excludes browser startup

The runner's starting RSS and startup timer are captured before session launch,
but the sampler starts only after `BrowserSession::start` has completed
(`examples/scorecard.rs:49-66`). Thus both reported runner peak and Chrome-tree
peak omit launch-time peaks, while `startup_ms` covers that omitted interval.
The report's unqualified `peak_rss_bytes` names and the ratified "peak Glass
RSS" wording (`docs/plan/analysis/best-in-class-browser.md:43-47`) do not disclose
this scope. This is acceptable only if renamed/documented as workflow peak;
otherwise sampling must include startup (with Chrome discovery handled
appropriately).

### P3 — non-blocking: adapter documentation overstates runner scope

The adapter contract asks for runner *process-tree* resources
(`benchmarks/adapters/README.md:8-10`), while the Playwright implementation uses
only `process.memoryUsage().rss()` for the Node process
(`benchmarks/adapters/playwright-scorecard.mjs:26-30`, `100-105`). The report's
own scope string correctly says "Node only," but the contract should match it
or the adapter should aggregate non-Chrome runner descendants.

## Verification reused and performed

The task records successful formatting, strict Clippy, all-target tests, live
normal/forced-wrong scorecard runs, browser smoke, and Node syntax validation.
I reused those recorded results. Focused independent checks passed:

- `cargo test --example scorecard` — 3 passed;
- `node --check benchmarks/adapters/playwright-scorecard.mjs`;
- `git diff --check 3837b7c^ 3837b7c`.

No implementation changes were made during this review.
