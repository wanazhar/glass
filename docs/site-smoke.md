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
  smoke-sites tests/fixtures/site-smoke-modern-v1.json
```
`polite` enforces each site's `robots.txt` and crawl-delay rules. URL-policy
denials and robots/runtime-policy denials are distinct:

- `classification: "url_policy_denied"` means the active URL policy rejected
  the requested navigation; recovery hint: `review_url_policy`.
- `classification: "robots_policy_denied"` means robots retrieval/status or
  path rules rejected navigation; recovery hint: `review_robots_policy`.

Neither denial authorizes a retry or bypass. Use `development` only when the
operator is authorized to bypass that robots gate.

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

A non-zero exit status means at least one site has `status: "failed"`. Policy
denials remain machine-readable in the report.

## Batch input

`batch` accepts a JSON file, stdin when the path is omitted, or `-` explicitly:

```console
cat batch.json | glass batch -
```

Inline JSON is not a positional argument. If a JSON object is supplied where a
file path is expected, Glass now explains that the input must be a file path or
stdin instead of returning an operating-system `No such file or directory`
error.
