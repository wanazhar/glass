# Live-site smoke testing

`smoke-sites` runs a bounded, read-only compatibility probe against a JSON
manifest of public sites. Each site gets an isolated incognito session, then
Glass performs:

1. navigation with a 30-second deadline;
2. compact observation;
3. structured `inspectPage` observation;
4. a side-effect-free preflight of the first observed target, or the manifest's
   explicit `target`; and
5. a second structured observation to record revision continuity.

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
target published by `inspectPage`. A configured target that cannot be resolved
or is not actionable makes that site `failed`. A page with no interactive target
is reported as `partial`, not as a browser failure. A bounded inspection timeout
is also reported as `partial`, preserving the successful navigation and compact
observation evidence.

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

`polite` enforces each site's `robots.txt` and crawl-delay rules. A site that
returns a non-successful `robots.txt` response is reported as
`classification: "policy_denied"`; it is not silently treated as a browser
failure. Use `development` only when the operator is authorized to bypass that
robots gate.

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
