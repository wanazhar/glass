# Deterministic adversarial fixture

Status: complete locally.

This slice added the fixture manifest, typed controls and fault kinds, and an
opt-in browser smoke test. The fixture remains independent of Glass runtime
code and exposes a counted submit side effect plus state snapshots. Scenario
steps can apply a declared fixture control, and the manifest rejects controls
or faults it does not expose.

Implemented in:

- `tests/fixtures/reliability-lab.html`
- `tests/fixtures/reliability-fixture-v1.json`
- `docs/schema/reliability-fixture-v1.schema.json`
- `tests/browser_smoke.rs`

Validation: the reliability fixture and scenario integration tests. The
browser test additionally runs with `GLASS_E2E=1` and a discoverable Chrome.
