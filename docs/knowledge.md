# Persistent browser knowledge

Glass can store a small local set of bounded page observations. Knowledge is
optional and advisory.

Glass always collects a fresh observation before it uses stored knowledge.
Stored data never provides a current target reference. Stored data never
authorizes a mutation.

## Stored data

A record is scoped by:

- origin and path pattern;
- anonymous, authenticated, or profile-bound session scope;
- optional profile, locale, and tenant keys;
- browser family and version range;
- Glass schema version; and
- policy preset.

The runtime stores page-family records and hashed target-fingerprint records.
The contract also supports region models, route transitions, workflow entry
points, verified postconditions, extraction shapes, and invalidation rules.

Records do not contain CDP references, route handles, accessible names, raw DOM,
screenshots, cookies, credentials, or form values. Glass bounds records and
rejects sensitive field names.

## Record lifecycle

A record can be:

- `candidate`;
- `observed`;
- `verified`;
- `stale`;
- `contradicted`; or
- `quarantined`.

Fresh verification is required before promotion to `verified` or recovery from
`contradicted` and `quarantined`.

## Local store

The default store is:

```text
<config>/glass/knowledge/<profile>.json
```

Use `--knowledge-store PATH` to select another file.

Glass uses a sidecar lock, a temporary file, flush, and rename for writes.
Glass prunes records within bounded limits. Corrupt JSON returns an error. Glass
does not replace corrupt data silently.

## CLI

These commands do not start Chrome:

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

`explain` reports scope, provenance, lifecycle history, invalidation rules,
and the content hash. It states that a fresh observation is required.

## Knowledge-assisted operations

MCP provides two explicit operations:

- `observeKnowledge` collects a fresh semantic observation. With
  `freshOnly: true` it does not read the store. Otherwise it returns bounded
  scope and freshness assessments.
- `resolveIntentWithKnowledge` resolves current targets and may add
  `historicalMatch` evidence for an eligible target fingerprint.

Historical data cannot create a candidate. It cannot raise confidence. It
cannot bypass the guarded execution boundary.

Ordinary `observe` and `resolveIntent` do not read the knowledge store.

## Contract and scorecard

The snapshot contract is
[knowledge-v1.schema.json](schema/knowledge-v1.schema.json).

Run the local scorecard:

```console
cargo run --example knowledge_scorecard
```

The scorecard is local evidence. It is not a public release claim.

## Migration certification

The 0.2.x knowledge snapshot remains schema v1, so 0.2.1 uses an explicit
no-op migration boundary. Certify that boundary with:

```console
python3 scripts/check-knowledge-migration.py \
  --binary target/release/glass \
  --output knowledge-migration.json
```

The check round-trips the checked-in six-record corpus, rejects schema v2, and
verifies that rejected input leaves the existing store unchanged.
