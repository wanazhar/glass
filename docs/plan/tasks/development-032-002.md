---
id: development-032-002
scope: project runtime protocol adapters
status: completed
depends-on: [development-032-001]
---

# Objective

Expose the project runtime through a shared typed CLI command family and MCP
tools. All mutations remain local, bounded, workspace-confined, and actor
attributed. External agents must be able to inspect and operate the runtime
without going through the embedded TUI harness.

# Context

- `docs/plan/analysis/release-032.md`
- `docs/cli.md`
- `docs/mcp.md`
- `docs/ownership.md`
- `SECURITY.md`

# Path

- `src/cli/args.rs`
- `src/cli/runner.rs`
- `src/mcp/server.rs`
- `docs/cli.md`
- `docs/mcp.md`

# Verification

- parser tests for every project operation and invalid path/input;
- CLI JSON output tests against the real core;
- MCP initialize/list/call tests with strict newline JSON-RPC framing;
- `cargo test --locked` and Clippy.
