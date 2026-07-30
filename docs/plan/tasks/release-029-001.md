id: release-029-001
scope: 0.2.2 contract foundations
status: done
depends-on: []

# Objective
Deliver the canonical cross-interface failure/recovery contract, compact response modes, bounded local result artifacts, tolerant protocol fixtures, and effective capability agreement required before high-level operations are added.

# Context
- `issue://wanazhar/glass/29`
- `docs/plan/analysis/release-029.md`
- `docs/action-contract.md`
- `docs/schema-compatibility.md`
- `src/capabilities.rs`
- `src/protocol.rs`

# Path
- `src/` protocol, error, result, and capability modules
- `src/cli/` serialization helpers
- `src/mcp/` response projection
- `tests/fixtures/` compatibility fixtures
- focused unit and conformance tests

# Verification
- old-client/new-runtime additive response fixture parses;
- new-client/old-runtime fixture fails only on incompatible versions;
- every mutation failure exposes mutation possibility and retry classification;
- minimal/normal/diagnostic projections preserve recovery-critical fields;
- local result artifacts are bounded, redacted, atomically written, and prunable;
- `cargo test --all-targets --locked` and targeted protocol tests pass.
