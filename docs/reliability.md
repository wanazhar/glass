# Reliability laboratory

The reliability laboratory is a local release-checking tool. It stores scenario
definitions, fixture controls, independent side-effect oracles, and replay
evidence with the source code.

Supported release targets are Linux x86-64, Linux arm64, macOS x86-64, and
macOS arm64.
Windows is unsupported.

## Contracts

| Contract | Purpose |
|---|---|
| Scenario v1 | Define fixtures, operations, expected state, counters, forbidden outcomes, and budgets. |
| Fixture v1 | Define deterministic controls, faults, and independent oracles. |
| Replay v1 | Bind redacted events and observations to one scenario and environment. |
| Capability suite v1 | Group scenarios for matrix validation. |

The schemas are in [docs/schema](schema/reliability-scenario-v1.schema.json).

The checked-in fixtures are:

- `tests/fixtures/reliability-scenario-v1.json`;
- `tests/fixtures/reliability-fixture-v1.json`;
- `tests/fixtures/reliability-lab.html`; and
- `tests/fixtures/reliability-submit.json`.

The fixture tests target replacement, renaming, duplication, reordering,
movement, overlays, frame detachment, delayed effects, and counted submit
effects. The fixture does not use Glass decision logic.

Validate the checked-in deterministic matrix and write its inventory report:

```console
python3 scripts/check-reliability-matrix.py reliability-matrix.json
```

This validates scenario coverage, target rows, workflow sources, forbidden
outcomes, and independent fixture oracles. The inventory explicitly reports
`runtime_certification: not_run`; it does not turn static contract validation
into runtime certification.

## Run a scenario

Set up a local fixture server. Then run:

```console
glass certify run \
  --scenario scenario.json \
  --fixture tests/fixtures/reliability-fixture-v1.json \
  --url http://127.0.0.1:8000/fixture.html \
  --workflow-root tests/fixtures \
  --output evidence.json
```

The runner:

1. validates the manifest;
2. navigates to the local fixture;
3. runs only allowlisted fixture controls;
4. writes redacted evidence; and
5. reports the independent oracle result.

The runner rejects workflow sources outside `--workflow-root`.

The capability-suite smoke test can persist its redacted evidence by setting
`GLASS_RELIABILITY_EVIDENCE_DIR`. Native CI stores one six-file directory per
release target. Merge those directories and run the existing release gate once
per target:

```console
python3 scripts/merge-reliability-evidence.py \
  evidence/reliability-matrix.json evidence/reliability
python3 scripts/certify-reliability-matrix.py \
  evidence/reliability-matrix.json \
  --scenarios tests/fixtures/reliability-capability-suite-v1.json \
  --binary target/release/glass \
  --version 0.2.1 \
  --output evidence/reliability-scorecards.json
```

The merger requires all six scenarios, both redacted replay and independent
oracle evidence, and a complete clean run on each of the four native targets.
Only the second command may report `runtime_certification: certified`.

Renderer and browser disconnect probes are unsupported for release
certification when the browser cannot provide a complete post-fault oracle.
A denied raw-CDP path is also non-certifying.

Run the browser smoke tests:

```console
GLASS_E2E=1 cargo test --test browser_smoke reliability_lab_controls
GLASS_E2E=1 cargo test --test browser_smoke reliability_runner_generates_live_fixture_evidence
```

## Offline commands

Inspect a plan without Chrome:

```console
glass certify plan \
  --scenario tests/fixtures/reliability-scenario-v1.json \
  --fixture tests/fixtures/reliability-fixture-v1.json
```

Validate a replay:

```console
glass certify replay \
  --scenario tests/fixtures/reliability-scenario-v1.json \
  --input replay.json
```

Compare two replays:

```console
glass certify replay-diff \
  --scenario tests/fixtures/reliability-scenario-v1.json \
  --before baseline.json --after candidate.json
```

Evaluate release evidence:

```console
glass certify release \
  --version 0.2.0 \
  --scenarios scenarios.json \
  --observations observations.json \
  --replays replays.json
```

The release command fails closed when:

- a scenario is missing;
- a hash does not match;
- oracle or artifact evidence is incomplete;
- a budget is invalid;
- a forbidden outcome is present;
- a run fails, is indeterminate, or is unsupported; or
- a replay does not match its observation.

A safe refusal passes only when the declared terminal state and independent
oracle agree.

## Limits

The laboratory uses local deterministic fixtures. It does not discover public
sites. It does not publish a scorecard.

Use the [read-only real-site procedure](reliability-real-site.md) for manual
read-only evidence. Do not put credentials, cookies, page values, or
unredacted traces in evidence bundles.
