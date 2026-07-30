id: release-029-003
scope: 0.2.2 bounded agent operations
status: pending
depends-on: [release-029-001, release-029-002]

# Objective
Expose `inspect_page`, `find_target`, `act_and_verify`, `extract_structured`, and `recover_run` through Rust, CLI JSON, and MCP while reusing the guarded runtime.

# Context
- `issue://wanazhar/glass/29` sections 2, 5, and 6
- `docs/action-contract.md`
- `docs/semantic-observation.md`
- `docs/intent-resolution.md`
- `docs/knowledge.md`
- existing session action, semantic, checkpoint, workflow, and MCP modules

# Path
- high-level operation contracts and session implementations
- CLI and MCP tool registration/dispatch
- TypeScript/Python protocol types where needed
- deterministic browser-free fixtures and live fixture integration tests

# Verification
- every operation is bounded by time/output/collection/retry/action budgets;
- ambiguous targets never execute;
- stale revisions never dispatch;
- successful actions require requested postcondition verification;
- post-dispatch uncertainty is distinct and never recommends blind replay;
- structured extraction reports field errors, provenance, and explicit continuation bounds;
- equivalent CLI/MCP/Rust results serialize identically.
