id: release-029-004
scope: 0.2.2 bounded state and templates
status: done
depends-on: [release-029-001, release-029-003]

# Objective
Add versioned redacted session snapshots with deterministic inspect/diff/purge commands and five reviewable workflow starter templates compiled by the existing authoring pipeline.

# Context
- `issue://wanazhar/glass/29` sections 7 and 8
- `docs/checkpoint*` and `docs/workflows.md`
- `src/browser/session/checkpoint.rs`
- `src/browser/session/authoring.rs`

# Path
- snapshot contract/store and CLI/MCP commands
- template source files, listing/init dispatch, and deterministic fixtures
- privacy and redaction tests

# Verification
- snapshots are read-only, versioned, bounded, deterministic, and secret-safe;
- forbidden default fields never appear;
- snapshot diffs and purge behavior are deterministic;
- five templates compile and contain semantic targets, explicit bounds, verification, and secret placeholders;
- no generated template uses fragile coordinate actions by default.
