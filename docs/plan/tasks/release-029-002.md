id: release-029-002
scope: 0.2.2 installation diagnostics
status: done
depends-on: [release-029-001]

# Objective
Complete `glass doctor`, add deterministic `glass doctor --json`, and add `glass mcp-config --client generic|claude-code|codex|print` without mutating real profiles by default.

# Context
- `issue://wanazhar/glass/29` sections 1 and 3
- `docs/installation.md`
- `docs/mcp.md`
- `src/cli/args.rs`
- `src/cli/runner.rs`
- `src/daemon.rs`

# Path
- CLI argument and dispatch modules
- doctor/config contract modules and tests
- installation and MCP documentation

# Verification
- missing browser, unwritable path, policy, capability, MCP stdout, daemon, and PATH findings have stable codes and severity;
- browser smoke is opt-in or isolated;
- generated MCP configuration contains the actual executable path and deterministic JSON;
- all required commands run without browser startup where documented;
- CLI and MCP conformance tests pass.
