---
id: workspace-031-001
scope: Glass Workspace identity and ownership foundation
status: ready
depends-on: []
---

# Objective

Define bounded, durable workspace identity and actor/lease contracts so browser state, profile scope, semantic memory, workflows, presentation, and attachments can converge on one addressable Glass Workspace. This foundation must not start or mutate a browser yet.

# Context

- `docs/plan/analysis/release-031.md`
- `docs/architecture/README.md`
- `docs/daemon.md`
- `docs/knowledge.md`
- Issue #31 sections B–I, workspace amendments, and Experience Layer Gate
- `src/daemon.rs`
- `src/browser/session/profile.rs`

# Path

- new `src/workspace.rs` or `src/workspace/` module
- focused tests beside the module
- module-local contract doc if needed

Do not edit `src/lib.rs`, daemon dispatch, profile/session code, CLI/MCP, or TUI in this foundation task.

# Contract

Provide bounded, serde-stable types for:

- workspace IDs, aliases, lifecycle states, profile/privacy modes, and durable/ephemeral scope;
- stable `glass://` resource references for workspace/browser/target/run/revision/entity/memory/workflow/replay resources;
- actor roles Human, Agent, Observer and attachment identity/capabilities;
- mutation lease states and revision-guarded acquire/release/takeover decisions;
- ownership boundaries distinguishing workspace, browser, presentation, and external attachment ownership;
- typed invalid-reference, scope, lease, stale-revision, and lifecycle errors.

IDs and aliases must be bounded and normalized. References must not silently cross workspace/profile scope. Observers cannot obtain mutation authority. Disconnects must not implicitly close a workspace.

# Verification

Run focused workspace contract tests covering reference parsing/round trips, scope isolation, lifecycle transitions, lease arbitration, takeover/release, stale revision rejection, and bounded validation. Do not run formatters, linters, or project-wide test suites. Commit with `feat(workspace): ...` before handoff.
