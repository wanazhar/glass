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

---

## Adversarial popup-click review: `116740b` + `b4e50d8`

Reviewed the complete `b298edf..b4e50d8` diff against the approved
`click_expect_popup` contract. Commit `c751823` remains an ancestor of the
milestones.

### Findings

1. **P1 — blocking — the trusted-click witness is forgeable by page code.**

   The witness payload is a predictable process-local `AtomicU64` starting at
   one (`src/browser/session.rs:950`, `src/browser/session.rs:1453`,
   `src/browser/session.rs:2865`). `Runtime.addBinding` exposes
   `globalThis.__glass_popup_witness` to the page, so page script can call the
   binding directly with the expected small integer without any trusted click
   (`src/browser/session.rs:2891-2901`). The listener is also installed through
   the page-overridable `element.addEventListener`; a hostile element/page can
   capture the callback and invoke it with an ordinary object whose
   `isTrusted` and `currentTarget` properties satisfy the check
   (`src/browser/session.rs:2904-2923`). The resulting `Runtime.bindingCalled`
   event is indistinguishable from the intended callback at
   `src/browser/session.rs:3099-3109`. Use an unguessable per-operation nonce
   and native, non-overridable EventTarget methods, and ensure direct binding
   calls cannot constitute witness evidence.

2. **P1 — blocking — exactly-one and no-event-loss are decided too early.**

   `wait_for_causal_popup` returns as soon as the registry contains its first
   matching target (`src/browser/session.rs:3127-3133`). A click that opens two
   page targets can therefore succeed after the first `Target.targetCreated`
   is processed but before the second event is delivered. Readiness then
   verifies only the selected candidate and never repeats the authoritative
   target-set, topology-sequence, or `event_loss_count` checks
   (`src/browser/session.rs:3151-3224`). Event loss or a second opener match
   during attach/readiness can likewise occur after the only assessment and
   still return success. Require bounded stabilization followed by a final
   authoritative live-target uniqueness and event-loss check immediately
   before success.

3. **P1 — blocking — cancellation does not clean up the witness or verification
   attachment.**

   Witness cleanup is an awaited statement after `perform_popup_click`; dropping
   the operation future skips it (`src/browser/session.rs:2870-2880`). The page
   listener then survives until its five-second timer, the Runtime binding is
   never removed, and `popup_witness_sessions` retains every observed session
   ID without a declared bound (`src/browser/session.rs:2891-2922`). Similarly,
   after `Target.attachToTarget`, detach is only reached on normal control flow;
   cancellation while debugger-resume/readiness is pending leaves the attached
   session live (`src/browser/session.rs:3175-3224`). Replace both with
   cancellation-safe guards whose drops perform bounded cleanup, and bound or
   eliminate the retained session set.

4. **P1 — blocking — popup negative outcomes lose their typed contract at the
   MCP boundary.**

   `PopupClickErrorKind` is serializable, but `PopupClickError` itself is not
   (`src/browser/session.rs:832-858`). MCP error handling serializes only
   `TargetError`, `WaitTimeout`, and `PolicyError`; every popup-specific
   `ReleaseFailed`, `WitnessMissing`, `TopologyLagged`, `PopupMissing`,
   `PopupAmbiguous`, `PopupDestroyed`, `PopupOpenerMismatch`, or
   `PopupUnreadable` failure becomes the generic text `browser tool failed`
   (`src/mcp/server.rs:557-571`). Serialize a bounded popup error object and
   include it in the same typed MCP error path.

### Confirmed contract pieces

- Ordinary `click` still calls the unchanged `pointer_click` path and does not
  arm a witness (`src/browser/session.rs:2830-2833`).
- Only the locally constructed `CdpErrorKind::ResponseTimeout` on the explicit
  `mouseReleased` call is suppressible; protocol and transport errors become
  `ReleaseFailed` (`src/browser/cdp.rs:51-84`, `src/browser/cdp.rs:457-467`,
  `src/browser/session.rs:3002-3017`).
- The pre-release `Target.getTargets` snapshot is taken before release, target
  sequence and loss counters are monotonic/bounded, and ordinary topology
  discovery does not select a popup (`src/browser/session.rs:3002-3007`,
  `src/browser/session.rs:3050-3085`, `src/browser/session.rs:5016-5031`).
- Normal-path popup liveness, temporary attach, debugger resume, readiness,
  and detach calls are bounded and do not mutate Glass's active route
  (`src/browser/session.rs:3151-3236`). The returned success object includes
  explicit witness, release-acknowledgement, sequence, attachment, and
  ready-state evidence (`src/browser/session.rs:3027-3047`).
- CLI command parsing/dispatch and MCP list/schema/parser/dispatch are wired to
  the same `BrowserSession::click_expect_popup` operation. The scorecard now
  calls that operation and checks its causal flag/opener before selecting the
  popup (`src/cli/args.rs:113-119`, `src/cli/runner.rs:96-104`,
  `src/mcp/server.rs:606-612`, `src/mcp/server.rs:750-756`,
  `src/mcp/server.rs:881-889`, `examples/scorecard.rs:261-275`).

### Focused verification

- `cargo fmt --all -- --check` — passed.
- `cargo test --locked popup_` — passed 7 focused tests.
- `cargo test --locked click_expect_popup` — passed 2 focused tests.
- `cargo test --locked protocol_errors_are_not_typed_as_response_timeouts` —
  passed.
- `cargo test --locked mcp_dialog_actions_await_browser_completion` matched no
  test, but `git merge-base --is-ancestor c751823 b4e50d8` returned success,
  confirming the retained dialog fix is in history.
- No full acceptance run was performed. Existing retained reports predate the
  new operation and are not runtime proof for this contract.

### Verdict

**blocked**

The surface is complete and the timeout distinction is sound, but the current
witness can be forged, uniqueness/loss can race after assessment, cancellation
leaks state, and MCP erases the required typed failures. It is not safe to
rerun or rely on the full acceptance gate until these blockers are fixed.

---

## Popup hardening re-review: `5469e71` + `be06e4e` + `406a49f`

Reviewed the complete hardening delta against the popup contract in
`docs/architecture/automation.md`, including the exact CSS-resolved backend
identity follow-up.

### Resolved findings

- The witness is private isolated-world state installed with captured native
  `EventTarget` and `Reflect.apply` functions. There is no page-visible binding,
  predictable nonce, or page-world callback to forge. Only an `isTrusted` click
  whose `currentTarget` is the exact resolved element can set the state.
- CSS resolution preserves identity. When the resolved element has only a
  frontend `nodeId`, `DOM.describeNode` translates that same node to its
  `backendNodeId`; the locator is not queried a second time. The witness then
  resolves that backend identity in the isolated world.
- Witness and popup attachment ownership use drop guards. Cancellation before a
  worker returns also drops the worker-owned guard, and remote witness state has
  a five-second self-cleanup fallback. The former unbounded witness-session set
  is gone.
- The release deadline is operation-local and exactly 500 ms. Ordinary CDP calls
  retain the client deadline, ordinary `click` still uses `pointer_click`, and
  only a typed response timeout on the explicit release enters recovery.
- Popup failures are bounded, serializable, and included in MCP's typed browser
  error path. Final discovery repeats authoritative live-target uniqueness and
  compares topology sequence and loss epoch without selecting the popup.

### Blocking finding

1. **P1 — bounded topology stabilization is not implemented, and a healthy
   integrated popup fails closed.**

   `wait_for_causal_popup` returns on the first valid candidate
   (`src/browser/session.rs:3079-3113`). After attach/readiness,
   `final_popup_verification` samples the current sequence once, immediately
   calls `Target.getTargets`, and rejects any sequence movement during that call
   (`src/browser/session.rs:3207-3276`). There is no bounded quiet/stabilization
   phase before the final authoritative check, despite the architecture contract
   requiring one. In the one-iteration real-Chromium scorecard, the valid popup
   failed with `TopologyLagged: popup topology changed during final authoritative
   verification` after about 567 ms. The popup-only runner passed because its
   fixture/timing did not expose this late topology delivery. Add a bounded
   stabilization loop that waits for topology sequence and loss epoch to remain
   unchanged for a declared quiet interval, then perform the existing final
   authoritative uniqueness/loss checks immediately before success. New target,
   destruction, ambiguity, or loss must still fail closed.

### Focused verification

- `cargo test --locked popup_` — passed 12 focused tests.
- `cargo test --locked click_expect_popup` — passed 2 focused tests.
- `cargo test --locked backend_identity` — passed 2 focused tests.
- `cargo test --locked protocol_errors_are_not_typed_as_response_timeouts` —
  passed.
- `GLASS_POPUP_BENCH_ITERATIONS=1 CHROME_PATH=/snap/bin/chromium cargo run
  --locked --release --example popup_benchmark` — passed with no failures;
  healthy ACK 0.77 ms and missing-ACK recovery 556.76 ms. One sample is
  explicitly not claim-eligible.
- `GLASS_SCORECARD_ITERATIONS=1 CHROME_PATH=/snap/bin/chromium cargo run --locked
  --release --example scorecard` — hard gate failed; popup was one of two
  failures and specifically failed on the final topology race above. The other
  failure was the pre-existing download expectation mismatch and is outside this
  popup review.

### Verdict

**blocked**

The earlier witness-forgery, identity, cancellation, timeout-isolation, bounded
state, and MCP typing blockers are resolved. The sole popup-contract blocker is
the missing bounded topology stabilization phase, now reproduced in the real
integrated scorecard. Do not treat the popup operation as acceptance-ready until
that scorecard popup scenario passes while retaining final fail-closed
uniqueness, destruction, and loss checks.

---

## Popup topology stabilization re-review: `7154989`

Reviewed the stabilization-only delta against the sole blocker above and reran
the focused and bounded real-browser checks.

### Findings

- The candidate enters a 50 ms quiet window inside the existing two-second
  evidence deadline. The sampled state contains both topology sequence and loss
  epoch; any change resets the quiet-window start.
- Each poll reruns the fail-closed topology assessment. Candidate replacement,
  multiple opener matches, destruction, or event loss therefore does not become
  a stabilization retry or success.
- A topology that never becomes quiet exits at the shared deadline as the typed,
  bounded `TopologyLagged` failure. The focused moving-topology test verifies
  both the deadline behavior and error kind.
- The prior final verification remains after attachment and readiness. It still
  performs authoritative `Target.getTargets` uniqueness, then immediately
  rejects any sequence/loss movement during that discovery and re-assesses the
  candidate before returning. No popup is selected as part of verification.
- The real integrated popup scenario that previously reproduced the race now
  succeeds as `popup-controlled`. This is the decisive regression evidence;
  the overall one-iteration scorecard remains red only because its unrelated
  download scenario reports `download-canceled` instead of `download-complete`.

### Focused verification

- `cargo test --locked popup_` — passed 14 focused tests.
- `cargo test --locked click_expect_popup` — passed 2 focused tests.
- `cargo test --locked backend_identity` — passed 2 focused tests.
- `cargo test --locked protocol_errors_are_not_typed_as_response_timeouts` —
  passed.
- `GLASS_POPUP_BENCH_ITERATIONS=1 CHROME_PATH=/snap/bin/chromium cargo run
  --locked --release --example popup_benchmark` — passed with no failures;
  healthy ACK 6.03 ms and missing-ACK recovery 676.01 ms. One sample remains
  explicitly not claim-eligible.
- `GLASS_SCORECARD_ITERATIONS=1 CHROME_PATH=/snap/bin/chromium cargo run --locked
  --release --example scorecard` — popup passed in 635.95 ms. The overall hard
  gate remained false solely for the out-of-scope download mismatch.

### Verdict

**pass**

The bounded stabilization blocker is resolved without weakening the final
authoritative uniqueness, destruction, or loss checks. The popup contract is
ready for the retained full acceptance rerun. This verdict is scoped to popup
hardening; it does not waive the independent download scorecard failure or
constitute a multi-iteration performance claim.

---

## Incognito download compatibility review: `3c5f8d7` + `1a28dd1`

Reviewed the compatibility bridge against its ownership/privacy contract and
ran focused protocol tests plus the bounded real-browser scorecard.

### Blocking findings

1. **P1 — CDP-created browser contexts are not excluded by the runtime gate.**

   `use_page_download_compatibility` receives only `self.chrome.is_some()` and
   `self.disposable_profile.is_some()` (`src/browser/session.rs:2793-2796`,
   `src/browser/session.rs:5602-5604`). Those values prove that the browser was
   launched by Glass with a disposable command-line-incognito profile, but they
   do not prove that the currently selected target belongs to that command-line
   off-the-record context. The public raw CDP client can create a browser
   context and target, and `select_target` can select it; topology does not retain
   or validate `TargetInfo.browserContextId`. The bridge would then send
   `Page.setDownloadBehavior` to a CDP-created context, contrary to the explicit
   contract. Capture and validate the selected target's authoritative context
   identity before enabling either scope, and fail closed or skip the bridge for
   every CDP-created context. Add a protocol test that creates/models a target
   with `browserContextId` and proves no Page fallback is issued.

2. **P1 — typed download failures are erased at the MCP boundary.**

   `DownloadError` is bounded and serializable, and library/CLI callers retain
   `AuthorizationFailed` or `RestorationFailed`. However, MCP's
   `typed_browser_error` handles `TargetError`, `WaitTimeout`, `PolicyError`, and
   `PopupClickError` only (`src/mcp/server.rs:580-603`). Both download-specific
   error kinds therefore become generic `browser tool failed` content for the
   shipped `download` tool. Include `DownloadError` in that serializer and add
   MCP coverage for both kinds.

### Confirmed contract pieces

- The happy-path bridge is limited to an owned session with a disposable
  command-line-incognito profile; attached, default, and persistent-profile
  sessions do not qualify under the current gate.
- The active top-level target session is copied before authorization and passed
  immutably into the guard. Page allow/deny calls use that captured session even
  if the active route later changes.
- Browser-domain `downloadWillBegin` and `downloadProgress` remain the only
  lifecycle evidence. Page behavior grants/denies compatibility permission but
  is not treated as completion evidence.
- Authorization is Browser allow followed by Page allow. Normal restoration and
  cancellation cleanup are Page deny followed by Browser deny. Partial Page
  enable restores Browser deny; partial Page restoration still attempts Browser
  deny; both normal failure paths return bounded typed errors.
- Guard acquisition is worker-owned, so cancellation during partial enable
  drops the completed guard and restores both scopes. The acquired guard also
  restores both scopes when the caller cancels the download wait.

### Focused verification

- `cargo test --locked download_` — passed 5 focused tests.
- `GLASS_SCORECARD_ITERATIONS=1 CHROME_PATH=/snap/bin/chromium cargo run --locked
  --release --example scorecard` — passed all 11 scenarios; download returned
  `download-complete` in 32.65 ms and the one-iteration hard gate passed. This
  bounded run is functional evidence, not a performance claim.
- No full acceptance run was performed.

### Verdict

**blocked**

The compatibility mechanism fixes the real command-line-incognito download and
its authorization/restoration lifecycle is substantially correct. It is not yet
contract-complete because it can cross into a CDP-created context and its typed
fail-closed errors do not survive MCP. Both are release-boundary issues even
though the owned-incognito happy-path scorecard passes.

---

## Incognito download boundary re-review: `ba1328b`

Reviewed the two P1 fixes against the preceding blocked verdict and reran the
focused protocol and bounded real-browser checks.

### Findings

- Startup captures the authoritative `browserContextId` from the exact original
  target only for a Glass-owned command-line-incognito launch. Missing,
  malformed, mismatched-target, or failed context lookup aborts startup after
  closing CDP and shutting down the owned browser.
- Before any `Browser.setDownloadBehavior` or `Page.setDownloadBehavior`
  mutation, the incognito acquisition path repeats `Target.getTargetInfo` for
  the captured selected target ID and requires its context ID to exactly equal
  the originally launched incognito context ID.
- A target in a raw-CDP-created context therefore fails closed as bounded typed
  `AuthorizationFailed`. The mismatch protocol test observes only the context
  lookup and zero behavior mutations. The matching test reaches the same
  captured page-session allow/deny lifecycle and preserves Page-deny before
  Browser-deny cleanup.
- Attached, default, and persistent-profile sessions still do not enter the Page
  compatibility path. The new identity validation narrows the prior owned plus
  disposable gate rather than broadening it.
- MCP's typed browser error serializer now includes bounded serializable
  `DownloadError`. Focused coverage verifies both `authorization_failed` and
  `restoration_failed` JSON content instead of generic tool failure text.

### Focused verification

- `cargo test --locked download_` — passed 8 focused tests, including context
  mismatch before mutation, matching lifecycle, partial enable/restoration, and
  cancellation cleanup.
- `cargo test --locked serializes_download_failures_as_typed_mcp_content` —
  passed.
- `GLASS_SCORECARD_ITERATIONS=1 CHROME_PATH=/snap/bin/chromium cargo run --locked
  --release --example scorecard` — passed all 11 scenarios; download returned
  `download-complete` in 42.06 ms and the bounded one-iteration hard gate passed.
- No full acceptance run was performed.

### Verdict

**pass**

Both P1 boundary failures from `34756ce` are resolved. The Page compatibility
bridge is now restricted to the exact originally launched incognito context
before mutation, raw-CDP context mismatch fails closed, the intended lifecycle
still completes, and typed failures survive MCP. The download compatibility
contract is ready for the retained full acceptance rerun; one iteration is not
a performance claim.

---

## Measured popup retry and MCP dialog adapter review: `d12d125` + `139e665` + `8ea8853`

Reviewed the popup retry contract, focused tests, the supplied 30-sample popup
report, and the public Playwright MCP dialog path plus its one-iteration report.

### Popup finding

1. **P1 — the shared evidence deadline is checked before, but not after, the
   final authoritative query.**

   `final_popup_verification` rejects an already-expired deadline at the top of
   its loop, then calls `Target.getTargets` through the independent two-second
   `popup_verification_call` timeout (`src/browser/session.rs:3303-3333`). If
   that query begins just before the evidence deadline, returns after the
   deadline, and the topology sequence and loss epoch remain unchanged, the
   function returns success without rechecking the shared deadline
   (`src/browser/session.rs:3370-3388`). This contradicts the documented rule
   that failure to settle before the existing evidence deadline remains typed
   failure. Bound the final query to the remaining shared deadline and/or
   recheck the deadline before the success return. Add a delayed stable-query
   test proving an after-deadline response cannot succeed.

The retry is otherwise appropriately narrow: authoritative discovery must still
contain exactly one later live opener match for the same candidate; any event
loss change fails `TopologyLagged`; topology re-assessment preserves ambiguity
and destruction failures; and only benign sequence movement under the same loss
epoch enters another quiet interval and query.

The supplied `/tmp/glass-popup-final-retry-30.json` reports 30/30 healthy ACK
samples and 30/30 missing-ACK recoveries with no failures, p95 recovery 605.84
ms, and no expectation breaches. It contains Chrome path and sample policy but
no git revision, generated timestamp, command, host identity, or artifact link.
It is useful measured diagnostic evidence but is not revision-bound acceptance
evidence and cannot by itself ratify `139e665`.

### MCP adapter findings

- The dialog action uses negotiated released public tools:
  `browser_click`, `browser_handle_dialog`, and `browser_evaluate`. Dialog
  acceptance is performed only by `browser_handle_dialog`; neither unsafe code
  nor evaluated JavaScript accepts the modal. Evaluation only reads the authored
  postcondition.
- Tool negotiation now explicitly requires `browser_handle_dialog` before the
  corpus starts. The adapter also retains negotiation for every other tool it
  uses, including the released unsafe runner used by non-dialog scenarios.
- Retry is bounded to 20 attempts with 25 ms inter-attempt delay, and every tool
  call is bounded by the configured MCP request timeout. It retries only the
  released server's specific modal-state rejection and fails other errors
  immediately. The message's 500 ms describes sleep budget rather than an exact
  wall-clock bound because tool-call time is additional, but total work remains
  bounded.
- The top-level `finally` closes the MCP child and recursively removes its unique
  temporary output directory. The client drains/rejects pending requests on
  failure and escalates from TERM to KILL after two seconds.
- `/tmp/glass-mcp-dialog-fixed.json` confirms the dialog scenario completed as
  `dialog-accepted` using Playwright MCP 0.0.78. The supplied trace file reflects
  the older unsafe dialog implementation and is not evidence for `8ea8853`.

### Separate newly exposed defect

The fixed one-iteration MCP report is not an acceptance pass: its download
scenario failed with `ENOENT` while copying the Playwright artifact into the
adapter output directory, yielding 10/11 successes and `hard_gate_passed: false`.
This is distinct from dialog completion and must be diagnosed and fixed before
the MCP adapter or aggregate acceptance gate is approved. No result was silently
synthesized or waived.

### Focused verification

- `cargo test --locked popup_` — passed 16 focused tests.
- `node --check benchmarks/adapters/playwright-mcp-scorecard.mjs` — passed.
- Reviewed `/tmp/glass-popup-final-retry-30.json`,
  `/tmp/glass-mcp-dialog-fixed.json`, and
  `/tmp/glass-mcp-dialog-trace.stderr` directly.
- No full acceptance run was performed.

### Verdict

**blocked**

The MCP dialog fix passes its scoped review, but adapter acceptance remains
blocked by the separate download `ENOENT`. The popup retry correctly preserves
loss, ambiguity, destruction, and identity checks, but its after-deadline stable
query can still succeed and the 30-sample report is not revision-bound. Fix and
test that deadline edge before approving the popup contract or rerunning the
full gate.

---

## Popup deadline and provenance re-review: `1e18bd9` + `2b74e94` + `6d18075`

Reviewed the remaining shared-deadline P1 and exercised the provenance fields
from a clean current-revision worktree.

### Findings

- Final authoritative discovery now computes the remaining shared evidence
  budget immediately before `Target.getTargets` and wraps that exact request in
  the remaining duration. Zero remaining time and query expiry both return
  bounded typed `TopologyLagged`; a protocol error remains typed
  `PopupUnreadable`.
- After parsing authoritative uniqueness and rechecking loss epoch, candidate,
  ambiguity, and destruction state, the stable-sequence success branch performs
  a second deadline check immediately before returning. A response that arrives
  at or after the deadline cannot succeed even if topology is otherwise stable.
- The delayed stable-query regression holds the response for 50 ms against a
  15 ms shared deadline and verifies typed `TopologyLagged`. Existing focused
  tests still cover benign final-query movement retry, perpetual movement at the
  shared deadline, event loss, ambiguity, destruction, and exact candidate
  identity.
- Popup benchmark reports now include UTC generation time, exact git revision,
  worktree-clean boolean, declared artifact path, reproducible program,
  arguments and environment, OS, architecture, host, Chrome version, Rust
  version, and Glass version alongside the existing sample policy and results.
- A bounded one-sample run from the clean worktree emitted revision
  `6d18075e4fc6393de2b046579d4ee57d33c9a732`, `git_worktree_clean: true`, host
  `redacted-host`, Linux/aarch64, Chromium 150.0.7871.46, Rust 1.97.0, the exact
  command environment, and `/tmp/glass-popup-provenance-check.json` as its
  artifact path. The report correctly marked one sample non-claim-eligible.

### Focused verification

- `cargo test --locked popup_` — passed 17 focused tests.
- `cargo test --locked --example popup_benchmark` — passed.
- `GLASS_POPUP_BENCH_ITERATIONS=1
  GLASS_POPUP_BENCH_ARTIFACT=/tmp/glass-popup-provenance-check.json
  CHROME_PATH=/snap/bin/chromium cargo run --locked --release --example
  popup_benchmark` — passed with no failures and complete clean-revision
  provenance.
- `git status --porcelain` — empty before and after the run.
- No full acceptance run was performed.

### Verdict

**pass**

The after-deadline success edge is closed and covered by a typed delayed-query
regression. The benchmark can now produce complete, clean, revision-bound raw
evidence. This approves the popup deadline/provenance fixes; it does not
retroactively make the older metadata-free 30-sample file revision-bound or
waive the separate MCP download `ENOENT` acceptance defect.

---

## Public MCP download evidence review: `5b93ee9` + `14246f8` + `2ceb457`

Reviewed the documented race diagnosis, public adapter parser, exact-artifact
follow-up, server trace, and supplied one-iteration result.

### Confirmed behavior

- The root-cause diagnosis matches the trace. The former
  `browser_run_code_unsafe` path waited for Playwright's download and called
  `createReadStream` while the released MCP server independently copied the same
  temporary artifact into its configured output directory. The resulting
  ownership race produced the observed `ENOENT`.
- The committed download scenario no longer uses unsafe runner code or direct
  Playwright APIs. It invokes the already negotiated public `browser_click` tool
  on the authored download link and waits for that bounded MCP request to
  complete.
- The released server trace shows `browser_click`, creation of
  `<unique-output-dir>/glass.txt`, and a response event stating that
  `glass.txt` was downloaded to that directory. The supplied one-iteration
  report completed all 11 scenarios, with download success in 543.01 ms, zero
  wrong actions, and `hard_gate_passed: true`.
- Adapter shutdown remains in the top-level `finally`: it terminates the MCP
  child with bounded escalation and recursively removes the unique output
  directory.

### Acceptance evidence

The initial `14246f8` predicate alone accepted the completed-event line without
binding its arbitrary textual destination suffix to the configured directory.
Follow-up `2ceb457` closes the acceptance gap by synchronously reading the exact
unique `${outputDir}/glass.txt` after the completed event and requiring its full
UTF-8 contents to equal the fixture bytes `glass`. A missing path, wrong path,
directory, unreadable file, or wrong content throws before scenario success.

The textual predicate is bounded by the client's one-MiB response ceiling and
requires a completed `glass.txt` line under an Events heading in the current
tool response. Text alone cannot forge success because the separately configured
unique output path must exist with exact fixture content. The adapter never
opens the server-owned temporary Playwright artifact; it reads only the completed
copy it owns and deletes that directory in `finally`.

### Focused verification

- `node --check` on the exact current adapter through `2ceb457` — passed.
- Reviewed `/tmp/glass-mcp-download-public.json` and
  `/tmp/glass-mcp-download-public.stderr`; the functional run passed 11/11 and
  the trace contains the real server-created output path.
- Reviewed `/tmp/glass-mcp-download-repro.json` and its trace for the original
  unsafe ownership race.
- No full acceptance run was performed.

### Verdict

**pass**

The unsafe artifact race is removed. Success now requires the negotiated public
click response's completed download event plus exact configured-path fixture
content, with bounded protocol handling and deterministic temporary cleanup. The
supplied one-iteration adapter result passes all scenarios. This scoped approval
does not replace the required multi-iteration acceptance rerun.

---

## MCP checkpoint invocation re-review: `904d685`

Re-reviewed the focused fix for stale same-revision checkpoint attribution.

### Confirmed behavior

- The runner removes the deterministic checkpoint path before spawning the
  adapter and creates a fresh cryptographic UUID plus UTC start time for every
  invocation.
- The exact invocation identity is passed to the adapter, written into every
  atomic checkpoint, and matched byte-for-byte during timeout validation in
  addition to the Git revision and controlled configuration.
- A timeout before the first checkpoint publication finds no file, so a stale
  checkpoint cannot be retained as current partial diagnostic evidence.
- The lifecycle regression pre-seeds a valid stale checkpoint, prepares a new
  invocation, verifies both fresh identity and unlinking, and confirms that a
  pre-publication timeout retains nothing.

### Focused verification

- `node --test benchmarks/tests/checkpoint.test.mjs` - passed 4 tests.
- `node --check benchmarks/checkpoint.mjs` - passed.
- `node --check benchmarks/run-acceptance.mjs` - passed.
- `node --check benchmarks/adapters/playwright-mcp-scorecard.mjs` - passed.
- No browser or full acceptance run was performed.

### Verdict

**pass**

The stale-checkpoint provenance blocker and its lifecycle-test gap are closed.
No new blocker was found in this bounded scope. Partial checkpoint evidence
remains diagnostic-only and cannot satisfy an acceptance or best-in-class gate.

---

## Comparative winner gate review: `6c8d29f` + `de79d7f`

Reviewed exact-matrix enforcement, Glass-only correctness and safety,
comparator outcome publication, task-success ranking, efficiency scope, and MCP
scenario-versus-transport failure handling.

### Confirmed behavior

- Every required adapter must have completed and supplied a report already
  validated as the exact corpus-by-iteration matrix.
- Comparator scenario failures and wrong actions remain published without
  becoming Glass correctness failures; Glass independently requires zero wrong
  actions and a perfect deterministic matrix.
- Glass must not trail either required comparator on task-success rate.
- Missing reports, null efficiency metrics, incomparable MCP scope, transport
  loss, and incomplete matrices fail their relevant gates closed.
- Ordinary MCP tool errors become scored scenario failures and continue through
  all requested rows, while transport failure aborts the adapter.

### Blocking finding

**P1 - resource-scope compatibility is accepted by textual prefix.**

`comparableRunnerScope` accepts any Playwright scope beginning with
`Runner RSS is Node only; Chrome process-tree RSS`. A contradictory declaration
such as one continuing with `is included` therefore maps to the same scope as
Glass and can create the strict efficiency win even though the measurements are
not comparable. The report validator only requires a non-empty scope, so this
is reachable from an otherwise schema-valid report. Eligibility must use an
exact versioned scope identifier or exact allowlisted declaration, not a prose
prefix. The efficiency check should also require positive peak RSS values;
currently finite zero is accepted as a winning measurement.

Add regressions for a shared-prefix contradictory scope and zero-valued peak
RSS. Both must leave `glass_declared_efficiency_win` false.

### Focused verification

- `node --test benchmarks/tests/acceptance-gates.test.mjs
  benchmarks/tests/mcp-adapter-loop.test.mjs` - passed 7 tests.
- `node --check` passed for the gate, runner, MCP adapter, and both focused test
  files.
- No browser or full acceptance run was performed.

### Verdict

**blocked**

The comparative correctness, matrix, and MCP failure semantics pass review, but
the declared efficiency win remains fail-open for ambiguous or invalid scope
metadata and therefore cannot support best-in-class eligibility yet.
