# Optional integrations

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Web access

`web_search` and `web_fetch` are native Core tools. They require no subprocess
or extension setup. Search uses Exa's hosted MCP endpoint; fetch makes a direct
bounded HTTP request and extracts readable text. Configuration, bounds, local
development endpoints, citations, and offline fixtures are documented in
[Web search and fetch](web-tools.md).

## MCP tools, out of process

Enable `mcp-bridge` to run configured MCP servers through a Python subprocess:

```toml
extensions = ["python3 /path/to/glass-core/extensions/mcp-bridge/mcp_bridge.py"]
```

`~/.config/glass-core/mcp.json` lists stdio servers:

```json
{
  "servers": {
    "files": { "command": ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"] }
  }
}
```

Each tool appears namespaced as `mcp__<server>__<tool>` with its JSON Schema
passed through. Past 40 tools the bridge switches to `mcp_search` and
`mcp_call`, so a large server does not inflate every prompt. A server that
fails to start is skipped with a note on stderr; calls into a dead server
return an error string, never a hung turn. Transports: stdio and Streamable
HTTP (`{"type": "streamable-http", "url": …, "headers": {…}}`, HTTPS for
non-loopback endpoints); the deprecated HTTP+SSE transport is refused with a
pointer. Servers are negotiated with the stateless 2026-07-28 revision when
they support it — `server/discover` probed first, per-request `_meta` after —
and fall back to the classic `initialize` handshake for everything older.
Nothing spawns unless `mcp.json` asks for it.

## Agent Plugins

glass-core loads [Agent Plugins 1.0](https://agent-plugins.org), the
cross-vendor packaging standard for skills and MCP servers:

```sh
gc plugin add /path/to/plugin
gc plugin list --json
gc plugin remove PLUGIN_NAME
```

A plugin's `skills/` join the skill catalogue and its `mcp.json` servers load
through the MCP bridge. Install is
user-level, like extensions: a repository cannot add a plugin, so cloning an
untrusted repo changes nothing here. Validation follows the spec's failure
boundaries — unknown manifest fields are reported and ignored, a bad schema
or name rejects the plugin, and one broken skill or server never stops the
rest from loading. Plugin-relative paths (`./`, `${PLUGIN_ROOT}`,
`${PLUGIN_DATA}`) are containment-checked against the plugin root, and each
plugin gets a persistent data directory at `plugins/data/<name>/`. `plugin
list` marks a plugin whose tree changed since install.

## Verify after writes, opt-in

`extensions/verify` runs the project's verifier after commands that write
files and appends failures to the tool result the model reads:

```toml
extensions = ["python3 /path/to/glass-core/extensions/verify/verify.py"]
```

Configure `~/.config/glass-core/verify.json` — `{"command": "cargo test
--quiet", "timeout_s": 90, "debounce_s": 8}` — or let it detect
`Cargo.toml` (`cargo check --quiet`) or a `package.json` `typecheck` script.
The verifier runs through the host's bash tool, so the sandbox policy and
output caps apply; it runs at most once per debounce window; failures append
as `[verify exit N]` and the first success after a failure reports
`[verify ok]`. `/verify` shows the active command and last result. Other project commands require explicit configuration.

## Better edits, opt-in

`extensions/hashline` adds `hl_read` / `hl_edit`, which refer to lines by a
short content anchor rather than by re-typed text:

```
1:37f9b8e4|fn main() {
2:aa5282df|    println!("hi");
```

A stale anchor is rejected before anything is written, and a batch of edits is
validated against the original file before writing. See the
[hashline reference](../extensions/hashline/README.md) for anchor semantics.

## Outlining before reading, opt-in

`extensions/index` adds an `index` tool: a file's skeleton with exact line
numbers (`index {path: "src/agent.rs"}`), or the files containing a literal
plus their outlines (`index {query: "retry_backoff"}`):

```toml
extensions = ["python3 /path/to/glass-core/extensions/index/index.py"]
```

Reads dominate token spend, so the point is reading a range instead of a
whole file; the system prompt tells the model to index first. Regex
scanners for rust/python/go/js/ts (unknown types say so rather than guess),
ripgrep when available, 60 lines per file and 200 per call, path-contained
to the workspace.

## Compiler feedback after edits, opt-in

`extensions/lsp` puts a real language server's verdict into the tool result
the model is about to read:

```toml
extensions = ["python3 /path/to/glass-core/extensions/lsp/lsp.py"]
```

```json
{
  ".py": {"command": ["pylsp"], "timeout_s": 20},
  ".rs": {"command": ["rust-analyzer"], "timeout_s": 30}
}
```

in `~/.config/glass-core/lsp.json`. After a command writes a file of a
configured language, the server is started lazily, the file is opened, and
diagnostics are appended as `[lsp] path:line:col severity: message` (errors
first, 20 cap) — the original tool output is preserved. Servers stay warm
for the session and shut down on exit. An explicitly requested `lsp {path}`
tool and a `/lsp` status command exist for the warm-up case; a server that
cannot start is reported once and then ignored, because a missing language
server must never break the edit loop.

Screenshots can be read through native [Image viewing](tools.md#image-sharing-and-viewing).
