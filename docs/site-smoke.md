# Live-site smoke testing

`smoke-sites` runs a bounded, read-only compatibility probe against a JSON
manifest of public sites. By default each site gets a cold, isolated incognito
session. Glass performs:

1. bounded session startup (reported separately from page work);
2. navigation with a 30-second deadline;
3. `observeBootstrap` as the first page read;
4. `inspectPage` only when a configured target must be resolved, an automatic
   target probe is requested, or richer semantic evidence is needed;
5. a side-effect-free preflight of the configured target or first observed
   target; and
6. a second structured observation to record revision continuity.

State-only pages (for example a challenge, consent interstitial, login page,
access-denied page, or empty page) do not trigger a full inspection. They are
reported with a page-state classification separate from the operation
classification.

Challenge evidence is checked immediately from bounded navigation metadata. A
known interstitial title such as `Just a moment...` is reported as
`pageState: "challenge"` and `classification: "challenge_interstitial"` with
`status: "partial"`; it does not trigger inspection, target preflight, or
recovery.

No clicks, form submissions, downloads, screenshots, logins, or JavaScript
evaluation are performed.

## Manifest

The input is a JSON file with up to 32 sites:

```json
{
  "schemaVersion": 1,
  "sites": [
    {"id": "github", "url": "https://github.com/"},
    {"id": "docs", "url": "https://developer.mozilla.org/en-US/", "target": "role=link;name=References"}
  ]
}
```

Each site may also include additive expectations:

```json
{
  "id": "docs",
  "url": "https://developer.mozilla.org/en-US/",
  "expectedOrigin": "https://developer.mozilla.org",
  "expectedPageState": "normal",
  "allowRedirect": false
}
```

`expectedOrigin` must be an absolute HTTP(S) origin, and `expectedPageState`
must be one of `normal`, `challenge`, `consent`, `accessDenied`,
`loginRequired`, `empty`, or `unknown`. `allowRedirect` defaults to the
legacy behavior when omitted; explicitly setting it to `false` reports a
bounded `classification: "expectation_mismatch"` if a redirect is observed
(or redirect evidence is unavailable). Old manifests remain valid.

`target` is optional. When omitted, Glass probes the first bounded semantic
target published by `inspectPage` when the bootstrap state calls for target
resolution. A configured target that cannot be resolved or is not actionable
makes that site `failed`. A page with no interactive target is reported as
`partial`, not as a browser failure. A detached or stale target is an
actionable `partial`: Glass may perform one fresh bootstrap plus inspection and
one read-only preflight retry. It never retries an action. A bounded
inspection timeout is also reported as `partial`, preserving successful
navigation and bootstrap evidence.

## Running a suite

Use a disposable browser and an explicit policy:

```console
cargo run -- \
  --incognito \
  --policy polite \
  --chrome-path /path/to/chromium \
  --viewport 1280x800 \
  smoke-sites crates/glass-browser/tests/fixtures/site-smoke-modern-v1.json
```
`polite` enforces each site's `robots.txt` and crawl-delay rules. URL-policy
denials and robots/runtime-policy denials are distinct:

- `classification: "url_policy_denied"` means the active URL policy rejected
  the requested navigation; recovery hint: `review_url_policy`.
- `classification: "robots_policy_denied"` means robots retrieval/status or
  path rules rejected navigation; recovery hint: `review_robots_policy`.

Neither denial authorizes a retry or bypass. Use `development` only when the
operator is authorized to bypass that robots gate.

The report includes additive `policyProvenance` metadata. Its
`robotsEnforced` boolean and `enforcement` value (`"enforced"` or
`"not_enforced"`) distinguish the polite robots gate from policies that do not
enforce robots rules. This is provenance only; it never authorizes a retry or
bypass.

`--stop-on-error` stops after the first site-level failure. Without it, all
manifest entries run and the command exits non-zero if any site fails.

## Result contract

The command emits one JSON report containing:

- per-site status: `passed`, `partial`, or `failed`;
- classification: policy denial, navigation timeout/error, observation error,
  target probe result, or metadata mismatch;
- final URL, title, ready state, and duration;
- step durations and bounded serialized response sizes; and
- region count, interactive target count, omission counts, target probe result,
  and post-observation revision continuity.
- requested and final URL, same-origin result, redirect count/evidence (with
  explicit `unknown` when bounded navigation evidence is unavailable);
- advisory page state (`normal`, `challenge`, `consent`, `accessDenied`,
  `loginRequired`, `empty`, or `unknown`) separately from operation
  classification;
- startup diagnostics, bootstrap/inspection/preflight/post-inspection timing,
  and bounded response bytes; and
- bootstrap, inspection, and post-inspection revisions with a consistency
  result. `revision_unstable` is always `partial` and includes the
  `reobserve_before_action` hint.

Startup timing describes session creation and is not folded into navigation
time. A cold isolated session is the default for public-site smoke. Reusing a
warm persistent MCP session can reduce startup latency, but the report still
separates startup from navigation and evidence checkpoints; callers must
re-observe after any target becomes stale. Recovery is read-only and bounded:
at most one fresh bootstrap/inspection and one preflight retry is attempted,
and no action is retried or authorized by bootstrap evidence.

### Structured navigation readiness

Every site result carries an additive `navigationReadiness` object when
navigation returns enough bounded evidence to report it:

```json
{
  "navigationReadiness": {
    "status": "complete",
    "phase": "lifecycle",
    "lifecycleComplete": true,
    "timeoutMs": 30000
  }
}
```

`navigationReadiness.status` is `complete` or `partial`;
`navigationReadiness.phase` is `document` or `lifecycle`. A `partial` result
means the navigation command returned and a bounded page-info checkpoint
succeeded, but the requested lifecycle condition did not complete before
`timeoutMs`. Partial readiness is evidence for reporting only: it never
authorizes a target resolution or action. Command, policy, or bounded page-info
failures remain site failures rather than partial readiness.

Bootstrap-first is deliberate. `observeBootstrap` is a bounded, page-state-only
checkpoint and never supplies action-authorizing targets. `inspectPage` is
requested only when a configured target, an automatic target probe, or richer
semantic evidence requires it. `preflight` is read-only and side-effect-free;
successful bootstrap evidence cannot be used to skip the current inspection or
preflight. A page-state classification therefore remains separate from the
operation `classification`.

### Timing and recovery contract

`startupDiagnostics` reports session creation separately from page work. Its
bounded fields are `launch_endpoint_ms`, `page_target_wait_ms`,
`cdp_connect_ms`, `target_attach_ms`, `event_setup_ms`, `policy_arm_ms`, and
`total_startup_ms`.
`steps` reports independently timed `startup`, `navigate`, `observeBootstrap`,
`inspectPage`, `preflight`, `preflightRetry`, `reobserve`, `reinspectPage`, and
`postInspectPage` operations when they occur. Each step has `name`, `status`,
`durationMs`, optional `responseBytes`, and a bounded optional `error`.
`durationMs` for the site is wall-clock end-to-end time; it must not be treated
as navigation latency.

Navigation timeout, inspection timeout, and a lifecycle-only partial readiness
are distinct outcomes. A navigation command that does not return a page and
cannot produce bounded page information is a failed navigation (for example
`navigation_timeout` or `navigation_error`), not a partial success. A returned
page with `navigationReadiness.status: "partial"` is reported separately and
requires the normal bootstrap-first checks. Inspection timeout preserves
navigation/bootstrap evidence and is `status: "partial"` with
`classification: "inspection_timeout"`.
When navigation returns a page with partial readiness, the operation
classification is `navigation_partial`.

The report is per-site provenance, not just an aggregate score. `requestedUrl`,
`finalUrl`, `sameOrigin`, `redirectCount`, and `redirectEvidence` identify the
bounded navigation observation; `redirectEvidence.status` is `observed` or
`unknown`, and `redirectEvidence.source` explains the evidence boundary.
`policyProvenance` contains `robotsEnforced`, `enforcement`, and `source`.
URLs and diagnostic text remain bounded and redacted as in the CLI contract.

Recovery is intentionally finite: a stale or detached target permits at most
one fresh `observeBootstrap` plus inspection and one read-only preflight retry.
No action is retried. `recoveryHint: "reobserve_before_action"` means the
caller must obtain fresh evidence; it is not permission to act.

### Interpreting page states and policy outcomes

`pageState` (`normal`, `challenge`, `consent`, `accessDenied`, `loginRequired`,
`empty`, or `unknown`) is advisory and must not be conflated with
`classification`. A challenge interstitial detected from bounded navigation
metadata is `pageState: "challenge"`,
`classification: "challenge_interstitial"`, and `status: "partial"`; it skips
inspection, preflight, and recovery. Consent, login, access-denied, and empty
states are reported for review and do not authorize an action.

`url_policy_denied` means the active URL policy rejected the requested URL.
`robots_policy_denied` means robots retrieval/status or path rules rejected it.
Both are fail-closed, carry their distinct recovery hints, and never authorize a
retry or policy bypass. A timeout is a runtime outcome, not a robots outcome.

### Coverage batches versus performance

Running many manifest entries concurrently can improve coverage throughput, but
the resulting batch is not a comparable performance baseline. Site content,
network conditions, browser startup churn, policy delays, redirects, challenge
rates, and scheduling differ per entry. Use the per-site fields above for
diagnosis; use the controlled local benchmark modes for performance claims.

A non-zero exit status means at least one site has `status: "failed"`. Policy
denials remain machine-readable in the report.

Expectation failures are machine-readable and bounded in each site result
under `expectationFailures`, with `kind`, `expected`, and `actual` fields.

## Batch input

`batch` accepts a JSON file, stdin when the path is omitted, or `-` explicitly:

```console
cat batch.json | glass batch -
```

Inline JSON is not a positional argument. If a JSON object is supplied where a
file path is expected, Glass now explains that the input must be a file path or
stdin instead of returning an operating-system `No such file or directory`
error.
