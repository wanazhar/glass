# Production canary runbook

A production canary is optional supplementary evidence. It must be read-only,
bounded, authorized by the site operator, and monitored. Deterministic local
fixtures remain the release gate.

## Preconditions

- Obtain approval for the exact public origin and time window.
- Use a health, documentation, or other non-sensitive page.
- Use an incognito session. Do not use a logged-in profile.
- Pin the `hardened` policy to the exact public host.
- Do not click, type, upload, download, evaluate JavaScript, import cookies,
  capture form values, or change browser emulation.
- Record Glass commit/version, browser version, OS/architecture, manifest,
  policy, and operator.

## Create one bounded manifest

Create `canary.json` with the approved URL and expected origin:

```json
{
  "schemaVersion": 1,
  "sites": [
    {
      "id": "approved-health",
      "url": "https://approved.example/health",
      "expectedOrigin": "https://approved.example",
      "expectedPageState": "normal",
      "allowRedirect": false
    }
  ]
}
```

Do not put credentials or secret query parameters in the manifest.

## Run the canary

Use one `smoke-sites` invocation so navigation, bootstrap observation,
read-only inspection/preflight, and post-observation continuity belong to the
same isolated browser session:

```console
glass --incognito \
  --policy hardened \
  --policy-allow-host approved.example \
  smoke-sites canary.json --stop-on-error
```

The command does not click, submit, download, screenshot, log in, or evaluate
JavaScript. Each site receives a cold isolated session. A target preflight is
side-effect-free.

## Interpret and record

Record the complete bounded report, exit status, elapsed time, final origin,
page-state classification, operation classification, omission/truncation
fields, revision continuity, and typed failure. A `partial` result is evidence
of a challenge, consent page, empty page, missing interactive target, or
bounded inspection problem; it is not a pass silently converted from missing
data.

Stop immediately on:

- outside-origin redirect or expectation mismatch;
- unexpected dialog, login, challenge, or access-denied state;
- URL-policy denial;
- transport/protocol error;
- stale target/topology that the bounded read-only recovery cannot reconcile;
  or
- any evidence that a mutation occurred.

Do not relax policy or retry with credentials to turn a canary green. Remove
the local manifest/report according to the operator's evidence-retention
policy.

## Claim boundary

One successful canary proves only that the recorded version/environment could
complete the bounded read-only scenario at that time. It does not certify all
site routes, authenticated workflows, mutation behavior, another browser, or
another platform. Compare canary evidence only when manifest, policy, browser,
environment, and page-state expectations are equivalent.

See [Live-site smoke testing](site-smoke.md), [read-only real-site
certification](reliability-real-site.md), and [Policy](policy.md).
