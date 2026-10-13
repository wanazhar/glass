# Optional extensions and helpers

Enable bundled extensions through `/extensions`, or configure their command in
`config.toml`. Enabling an extension starts executable local code and may require
an additional runtime. Review its access and configuration first.

| Component | Purpose | Runtime |
| --- | --- | --- |
| [Adapters](adapters/README.md) | Configured ACP harnesses and DAP debuggers | Python |
| [Browser](browser/README.md) | Screenshots and console verification through CDP | Python + Playwright |
| [Cache warmer](cache-warmer/README.md) | Optional idle prefix refresh requests | Python |
| [Codemode](codemode/README.md) | JavaScript over host tools | Node |
| [Hashline](hashline/README.md) | Content-anchored line edits | Python |
| [Index](index/README.md) | Source symbol outlines | Python |
| [LSP](lsp/README.md) | Configured language servers and diagnostics | Python + servers |
| [MCP bridge](mcp-bridge/README.md) | Configured MCP servers | Python + servers |
| [Memory](memory/README.md) | Separate Markdown memory index | Python |
| [Oracle](oracle/README.md) | Explicit alternate-model review | Python |
| [Subagent](subagent/README.md) | One-shot delegated tasks | Python |
| [Todo](todo/README.md) | Process-local task checklist | Python |
| [Verify](verify/README.md) | Project checks after detected shell writes | Python |
| [Web](web/README.md) | Readable pages and search results | Python |

The [companion](companion/README.md) and [structure](structure/README.md) directories
contain explicit CLI helpers rather than normal tool-handshake extensions.
`hello.py` is the protocol example. See [Extending glass-core](../docs/extending.md)
and [Permissions](../docs/security.md) for the host contract.
