# Reliability scenario contract

Status: complete locally.

This slice added the versioned scenario contract and checked-in JSON schema.
It validates platform labels, operation cardinality, budgets, expected
side-effect counters, and release-blocking forbidden outcomes. Scenario
content hashes bind later observations to the exact source definition.

Implemented in:

- `src/reliability.rs`
- `docs/schema/reliability-scenario-v1.schema.json`
- `tests/fixtures/reliability-scenario-v1.json`

Validation: `cargo test --lib reliability --locked` and the reliability
scenario integration test.
