# mcp-bridge

Make [Model Context Protocol](https://modelcontextprotocol.io) servers callable
from glass-core, out of process. The core binary keeps its "no MCP" size
argument; this bridge is an ordinary extension.

`~/.config/glass-core/mcp.json` (or `GC_MCP_CONFIG`):

```json
{
  "servers": {
    "files": { "command": ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"] },
    "notes": { "command": ["python3", "/path/notes_server.py"], "env": { "NOTES_DIR": "~/notes" } }
  }
}
```

- Every tool appears namespaced as `mcp__<server>__<tool>` with its JSON
  Schema passed through unchanged.
- Past **40 tools** the manifest switches to two meta-tools, `mcp_search` and
  `mcp_call`, so a large server does not inflate every prompt — the same
  deferred-tools idea as Claude Code's tool search.
- A server that fails to start is skipped with a note on stderr; a call into
  a dead server returns an error string, never a hung turn.
- Content arrays are flattened to text; non-text items become
  `[image content omitted]`-style placeholders rather than disappearing
  silently.
- stdio transport only. HTTP/SSE MCP servers are deliberately out of scope.

### Agent Plugins

The bridge also reads `mcp.json` from every installed
[Agent Plugin](https://agent-plugins.org) (`~/.config/glass-core/plugins/*/
mcp.json`), per the 1.0.0 spec: schema match required, `${PLUGIN_ROOT}` and
`${PLUGIN_DATA}` expanded in `args`/`env`/`cwd`, `./command` and `cwd`
containment-checked against the plugin root, and per-server failure
isolation. Plugin servers appear namespaced as `<plugin>.<server>` (so a
tool is `mcp__plugin.server__tool`), and each plugin gets a persistent
`plugins/data/<name>/` directory.

Startup is bounded: each server gets 8 seconds to initialize and list tools
before the bridge's own handshake has to complete, so a wedged server cannot
hold the session hostage. Offline check: `python3 scripts/smoke-mcp.py`.
