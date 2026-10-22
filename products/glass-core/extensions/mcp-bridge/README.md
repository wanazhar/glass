# MCP bridge extension

Connect configured Model Context Protocol servers through a Python subprocess.
Enable `mcp-bridge` in `/extensions`. Configuration uses
`~/.config/glass-core/mcp.json`, or `GC_MCP_CONFIG`:

```json
{"servers":{"files":{"command":["npx","-y","@modelcontextprotocol/server-filesystem","/tmp"]}}}
```

Review commands before enabling them. A command such as `npx -y` may download
third-party code; the server is executable local software with its applicable
capabilities and confinement.

## Tools and discovery

Tools use names such as `mcp__files__read_file`, retaining their server schemas.
Above 40 tools, the bridge exposes deferred search/call tools to bound the prompt
catalogue. Startup failure is reported per server. Non-text content uses explicit
placeholders when it cannot be represented.

## Transport and protocol

The bridge supports stdio and configured Streamable HTTP. Non-loopback HTTP
endpoints require HTTPS. Authentication failures identify missing credentials.
The deprecated HTTP+SSE transport is refused.

The bridge probes `server/discover` for its supported stateless revision and falls
back to classic `initialize`/`initialized` negotiation on method-not-found. Server
features remain subject to the bridge's supported protocol methods; unsupported
multi-round input requirements return an explicit result.

Each configured server has a bounded startup period, including initialization and
tool listing. An unavailable server does not block startup indefinitely. The host
can present supported elicitation forms or URL consent; headless requests requiring
user input fail explicitly.

## Plugins

Installed plugins can supply `mcp.json`. Servers use plugin-qualified names, and
`${PLUGIN_ROOT}` / `${PLUGIN_DATA}` resolve to validated local paths. Each plugin
retains its own persistent data directory. Invalid components are reported without
activating them. See [Integrations](../../docs/integrations.md#agent-plugins) and
[Permissions](../../docs/security.md).

Offline check: `python3 scripts/smoke-mcp.py` from the repository root.
