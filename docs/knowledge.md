# Persistent browser knowledge

Glass can keep a small local store of bounded observations about recurring
browser pages. Knowledge is optional and advisory. A fresh observation is
always collected before an assessed observation or knowledge-backed intent
resolution; stored records never provide a current element reference and never
authorize a browser mutation.

## What is stored

Records are versioned and scoped by:

- origin and path pattern;
- anonymous, authenticated, or profile-bound session scope;
- optional profile, locale, and tenant keys;
- browser family and optional version range;
- Glass schema version and policy preset.

Supported record kinds include page families, region models, target
fingerprints, route transitions, workflow entry points, verified
postconditions, extraction shapes, and invalidation rules. The current runtime
constructs page-family records and hashed target-fingerprint records from fresh
semantic evidence. Record payloads are bounded, reject sensitive field names,
and do not retain CDP references, route handles, accessible names, raw DOM,
screenshots, cookies, credentials, or form values.

Records use `candidate`, `observed`, `verified`, `stale`, `contradicted`, or
`quarantined` lifecycle states. Promotion to verified and recovery from
contradiction or quarantine require fresh verification. Fresh verification also
refreshes provenance timestamps and counts.

## Local store

The default file is profile-scoped under the platform configuration directory:

```text
<config>/glass/knowledge/<profile>.json
```

Use `--knowledge-store PATH` to select another file. Writes use a sidecar lock,
same-directory temporary files, flush-and-rename replacement, and bounded
pruning. Corrupt JSON is reported as an error and is never silently replaced.

The CLI management commands do not start Chrome:

```console
glass knowledge list
glass knowledge show RECORD_ID
glass knowledge explain RECORD_ID
glass knowledge stats
glass knowledge export [PATH]
glass knowledge import SNAPSHOT.json
glass knowledge invalidate RECORD_ID stale
glass knowledge purge https://example.test
```

`explain` reports scope, provenance, lifecycle history, invalidation rules, and
the canonical content hash. It also makes clear that a fresh observation is
still required.

## Knowledge-assisted observation and intent

MCP exposes two explicit opt-in operations:

- `observeKnowledge` always performs a fresh semantic observation. With
  `freshOnly: true` it does not open or read the store. Otherwise it returns
  eligible, stale, and out-of-scope assessments with bounded explanations.
- `resolveIntentWithKnowledge` resolves against current semantic targets and
  may add `historicalMatch` evidence when an eligible target fingerprint
  matches. Historical data cannot create a candidate, increase its confidence,
  or bypass the guarded execution boundary.

The ordinary `observe` and `resolveIntent` operations do not consult the
knowledge store implicitly.

## Contract and fixtures

The machine-readable v1 snapshot contract is in
[`schema/knowledge-v1.schema.json`](schema/knowledge-v1.schema.json). The
offline assessment corpus is in
[`../benchmarks/scenarios/knowledge-v1.json`](../benchmarks/scenarios/knowledge-v1.json)
and can be checked with:

```console
cargo run --example knowledge_scorecard
```
