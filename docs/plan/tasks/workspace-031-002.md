---
id: workspace-031-002
scope: Workspace coordinator and lifecycle integration
status: pending
depends-on: [workspace-031-001]
---

# Objective

Connect workspace identity to persistent profiles, browser sessions, memory scope, lifecycle, and attachments through one coordinator. Provide safe named workspace operations without exposing raw profile/port/process management in normal flows.

# Context

- `docs/plan/analysis/release-031.md`
- Issue #31 workspace and Experience Layer sections
- `src/workspace.rs` from `workspace-031-001`
- `src/daemon.rs`
- `src/browser/session/profile.rs`
- `src/browser/session/mod.rs`
- `src/browser/session/knowledge_store.rs`

# Path

Workspace coordinator/storage, profile ownership/locking, daemon integration, resource references, lease persistence, focused CLI/MCP/TUI attachment hooks, and docs. Preserve caller-owned browser state on presentation disconnect.

# Verification

Use real local persistence tests for create/open/list/clone/reset/suspend/resume/delete, concurrent scope isolation, lease ownership, and crash/recovery boundaries. No silent cross-profile reuse or untyped lifecycle fallback.
