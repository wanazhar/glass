---
id: mcp-bridge-001
scope: extensions/mcp-bridge, addons catalogue
status: done
depends-on: []
---
# Mcp bridge

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

An out-of-process MCP bridge: configured stdio MCP servers' tools become
callable from glass-core, namespaced, without the core binary growing an MCP
client. Ported from Crush/Codex/Copilot MCP support, placed where the
extensions ethos says capability belongs (research/harness-features-2026.md).

## Context

- Tools are declared at handshake as `mcp__<server>__<tool>` with pass-through
  JSON Schemas. Past 40 tools the manifest switches to `mcp_search` +
  `mcp_call` meta-tools so a large server cannot inflate every prompt.
- stdio transport only; HTTP/SSE is explicitly out of scope for v1.
- Startup is bounded (8s per server to initialize and list) because the glass
  handshake has its own deadline; an unresponsive server is skipped after its deadline.
- A dead server reports an error string per call; it never stalls a turn.

## Paths

- `extensions/mcp-bridge/mcp_bridge.py` (new): spawn/initialize/list/call,
  namespacing, deferred mode, content flattening.
- `extensions/mcp-bridge/README.md`, `crates/glass/src/addons.rs` (bundle).
- `scripts/mcp_fixture_server.py`, `scripts/smoke-mcp.py` (offline checks).

## Verification

- `scripts/smoke-mcp.py`: manifest namespacing, schema passthrough, call
  round-trip, error text, unknown-tool refusal, deferred search/call mode
  with a 45-tool fixture, malformed arguments.
- `cargo test -p glass-core` (catalogue entry) and Clippy.
