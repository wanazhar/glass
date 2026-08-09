# Read-only real-site certification

A real-site check is supplementary release evidence. It does not replace the
deterministic fixture suite.

The initial adapter inventory is
[`public-readonly-adapters.json`](public-readonly-adapters.json), with a
versioned contract at
[`public-readonly-adapters-v1.schema.json`](schema/public-readonly-adapters-v1.schema.json).
Validate its host allowlists, action denials, budgets, drift classifications,
and credential/mutation policy with:

```console
python3 scripts/check-public-readonly-adapters.py
```

This is static contract evidence only and reports `runtime_certification:
not_run`. An external-site run requires an approved route and must remain
supplementary to the deterministic suite.

Run a check only on an approved, read-only route.

## Conditions

- Use `--incognito` and the `hardened` policy.
- Allow only the exact host with `--policy-allow-host`.
- Do not provide credentials, cookies, API keys, or personal data.
- Use navigation, observation, text, bounded waits, and verification only.
- Do not submit, purchase, delete, upload, download, or dismiss consent.
- Set explicit duration and action limits.
- Store only redacted replay events and read-only oracle results.

Stop and classify the run as `unsupported` or `indeterminate` when the site
requires authentication, shows an unexpected destructive control, redirects
outside the allowlist, or does not provide complete oracle evidence.

These classifications cannot certify a release.

## Example

Use one `smoke-sites` manifest so navigation and evidence share an owned
session. Do not compose this check from independent one-shot `navigate`,
`observe`, and `verify` commands.

```json
{
  "schemaVersion": 1,
  "sites": [
    {
      "id": "approved-example",
      "url": "https://example.com/approved-read-only-route",
      "expectedOrigin": "https://example.com",
      "expectedPageState": "normal",
      "allowRedirect": false
    }
  ]
}
```

```console
glass --incognito --policy hardened \
  --policy-allow-host example.com \
  smoke-sites approved-sites.json
```

Confirm the exact manifest fields against `glass smoke-sites --help` and the
[site-smoke guide](site-smoke.md). This example does not certify `example.com`.
The current checkout does not discover approved sites or publish real-site
evidence.

## Live-site smoke is evidence, not a benchmark

The `smoke-sites` command is a bounded compatibility and safety probe. It is
not a performance regression gate, and a single run—whether it contains one
site or 50 sites—does not establish a regression or improvement. Coverage
batches may be run sequentially or in parallel for operational throughput, but
parallel scheduling, startup contention, network variance, robots delays,
redirects, and challenge pages make those batches non-comparable performance
baselines. Use the controlled local benchmark modes for performance work.

## Operator contract for a live run

Record the manifest revision, each site's redacted `requestedUrl`, the
`finalUrl`, `sameOrigin`, `redirectCount`, and `redirectEvidence`. The
top-level report also records `schemaVersion`, `policy`, `policyProvenance`,
`viewport`, `total`, `completed`, `passed`, `partial`, `failed`, and `sites`.
`policyProvenance` contains `robotsEnforced`, `enforcement`, and `source`; this
is provenance only and never grants permission to retry or bypass a policy.

Each site result separates session startup from navigation and evidence
checkpoints. `startupDiagnostics` contains bounded metadata fields
`launch_endpoint_ms`, `page_target_wait_ms`, `cdp_connect_ms`,
`target_attach_ms`, `event_setup_ms`, `policy_arm_ms`, and
`total_startup_ms`. The `steps` array records
the operation `name`, `status`, `durationMs`, optional `responseBytes`, and
bounded optional `error`. Startup time is not navigation time, and neither is
bootstrap or full inspection time.

Navigation may return structured readiness:

```json
{
  "navigationReadiness": {
    "status": "partial",
    "phase": "lifecycle",
    "lifecycleComplete": false,
    "timeoutMs": 30000
  }
}
```

`navigationReadiness.status` is `complete` or `partial`; `phase` is `document`
or `lifecycle`. Partial readiness is valid only after the navigation command
returned and bounded page information succeeded. It records useful evidence
while a lifecycle condition remained incomplete; it never authorizes target
resolution or an action. A command, policy, or page-info failure is an error,
not partial readiness. A caller must still perform the bootstrap-first flow and
obtain fresh evidence before any action.

The safe sequence is navigation, `observeBootstrap`, conditional `inspectPage`,
side-effect-free `preflight`, and a post-observation consistency check.
`observeBootstrap` is page-state evidence only and does not authorize a target.
Challenge, consent, login-required, access-denied, and empty states are
reported separately under `pageState`; they are not silently treated as normal
readiness. Never click, submit, download, dismiss consent, or evaluate
JavaScript as part of this probe.

## Distinguish denials, challenges, and timeouts

`url_policy_denied` means the active URL policy rejected the requested
navigation. `robots_policy_denied` means robots retrieval/status or path rules
rejected it. In `polite` mode, robots enforcement is expected; in a policy that
does not enforce robots, `policyProvenance.robotsEnforced` and
`policyProvenance.enforcement` make that fact explicit. Neither denial is
retryable or bypassable by the smoke runner.

Bounded challenge evidence is reported as
`pageState: "challenge"`, `classification: "challenge_interstitial"`, and
`status: "partial"`. It skips inspection, preflight, and recovery; operators
must not interpret it as a successful page check. A navigation timeout before a
page and bounded page information exist is a failed navigation. An inspection
timeout may preserve navigation/bootstrap evidence as `status: "partial"` with
`classification: "inspection_timeout"`. These outcomes are operationally
different from robots denials and challenge interstitials.

When navigation returns a page with partial readiness, the operation
classification is `navigation_partial`; this is reportable evidence, not
permission to continue into an action.

## Bounded recovery

Recovery is read-only and finite. A stale or detached target permits at most one
fresh bootstrap/inspection pass and one preflight retry. No browser action is
retried, and `recoveryHint: "reobserve_before_action"` is an instruction to
refresh evidence—not authorization to proceed. Keep the bounded `error`,
`recoveryHint`, `expectationFailures`, step statuses, and evidence sizes with
the per-site result so an unsupported or indeterminate site can be diagnosed
without replaying it.

## Benchmark mode boundaries

For performance evidence, label the run mode and keep controls identical:

- **cold isolated**: each sample creates and closes a new owned incognito
  session; startup is lifecycle/setup evidence;
- **warm persistent**: one owned session is reused, with cached observation
  explicitly distinguished from authoritative fresh observation; and
- **controlled attach**: an opt-in attach to the benchmark-owned Chrome
  endpoint, after ownership checks, measuring attach startup separately.

Do not compare cold, warm, and attach distributions as if they were the same
operation. Require bounded, claim-eligible samples, matched machine/Chrome/
fixture/lifecycle, and independent repeated runs before making a performance
claim. Public-site smoke timings include network and policy behavior and are
not interchangeable with the local fixture benchmark.
