---
id: workflow-019-039
scope: CLI and MCP workflow conformance
status: completed
depends-on: [workflow-019-038]
---

# Objective

Verify that the CLI and MCP adapters execute the same version-one workflow
contract and return the same result envelope as the Rust session API.

# Delivered

The real-browser frontend smoke test now submits one shared workflow through
CLI stdin and MCP `tools/call`. Both paths must return `completed` results with
trace event arrays before the test can pass.
