# Reliability laboratory

The reliability laboratory is a local development and release-checking
surface for Glass. It keeps scenario definitions, adversarial fixture
controls, independent side-effect oracles, and replay evidence versioned
alongside the code.

The current contracts support Linux x86-64 and macOS x86-64/arm64. Windows is
not part of the supported platform set.

## Current contracts

- [Scenario v1](schema/reliability-scenario-v1.schema.json) declares the
  fixture, setup, ordered operations, expected terminal state, side-effect
  counters, forbidden outcomes, and execution budgets.
- [Fixture v1](schema/reliability-fixture-v1.schema.json) names the checked-in
  deterministic controls, fault kinds, and independent oracles.
- [Replay v1](schema/reliability-replay-v1.schema.json) binds redacted ordered
  events and an observation to the exact scenario and records platform and
  browser budget metadata.
- [Capability suite v1](schema/reliability-capability-suite-v1.schema.json)
  packages multiple scenarios for deterministic matrix validation.

The checked-in examples are
`tests/fixtures/reliability-scenario-v1.json`,
`tests/fixtures/reliability-fixture-v1.json`, and
`tests/fixtures/reliability-lab.html`.

The fixture exposes target replacement, renaming, duplication, movement,
overlays, frame detachment, delayed effects, and a counted submit side effect.
It contains no Glass runtime logic. The browser smoke test is opt-in:

```console
GLASS_E2E=1 cargo test --test browser_smoke reliability_lab_controls
```

## Offline validation

Validate one replay bundle without starting Chrome:

```console
glass certify replay \
  --scenario tests/fixtures/reliability-scenario-v1.json \
  --input replay.json
```

Compare a baseline and candidate replay without exposing page values:

```console
glass certify replay-diff \
  --scenario tests/fixtures/reliability-scenario-v1.json \
  --before baseline.json --after candidate.json
```

Evaluate a complete release evidence set:

```console
glass certify release \
  --version 0.2.0 \
  --scenarios scenarios.json \
  --observations observations.json
```

The release gate fails closed when a scenario is missing, hashes do not match,
oracle or artifact evidence is incomplete, budgets are invalid, a forbidden
outcome is present, or the run is failed, indeterminate, or unsupported. A
safe refusal can certify only when its declared terminal state and independent
oracle agree with the scenario. The JSON result includes the detailed gate and
a category-level scorecard; the scorecard is derived from the same gate and is
not a separate source of truth.

## Boundaries

The current checkout validates contracts and exercises the local fixture; it
does not yet provide a browser-run scenario orchestrator, a real-site
certification workflow, or a public scorecard publisher. Those remain release
work for the 0.2.0 milestone. Do not place credentials, cookies, page values,
or unredacted traces in replay bundles.
