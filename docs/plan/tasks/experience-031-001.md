---
id: experience-031-001
scope: Glass Experience Layer entry points
status: pending
depends-on: [workspace-031-001, backend-031-001, presentation-031-001]
---

# Objective

Expose the integrated runtime through discoverable CLI/TUI/MCP entry points: a useful default `glass`, `doctor`, named workspace/resource commands, semantic inspect/diff/replay surfaces, polished typed output, and actionable errors. Do not duplicate behavior per interface.

# Context

- `docs/plan/analysis/release-031.md`
- `docs/INDEX.md`
- `docs/cli.md`
- `docs/mcp.md`
- `docs/daemon.md`
- Issue #31 Experience Layer amendment and Gate W

# Path

`src/main.rs`, `src/cli/args.rs`, `src/cli/runner.rs`, `src/tui/`, `src/mcp/`, shared result/error/resource renderers, docs and focused CLI/MCP/TUI tests. Use the contracts delivered by prerequisite tasks.

# Verification

Exercise non-interactive help/JSON output, interactive launcher entry, `doctor`, workspace/resource references, semantic fallback, actionable typed errors, and one shared result across CLI/MCP/TUI. Do not expose raw internal JSON as default human output.
