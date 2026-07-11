# MCP integration

Glass implements an MCP server over standard input and output. The server
starts a browser lazily on the first browser tool call.

## Configure a client

Build or install Glass, then configure the MCP client to execute the binary
directly with `--mcp`. A generic configuration looks like:

```json
{
  "mcpServers": {
    "glass": {
      "command": "/absolute/path/to/glass",
      "args": ["--mcp"]
    }
  }
}
```

Session flags may be included before `--mcp`, for example:

```json
{
  "command": "/absolute/path/to/glass",
  "args": ["--incognito", "--interaction", "fast", "--mcp"]
}
```

Use an absolute binary path in GUI clients, which may have a different `PATH`
from an interactive shell. Keep stdout reserved for MCP messages; Glass emits
diagnostics on stderr.

## Tools

| Tool | Important arguments | Result or effect |
|---|---|---|
| `navigate` | `url` | Navigate and return page state. |
| `click` | `target` or `selector` | Click one element. |
| `doubleClick` | `target` or `selector` | Double-click one element. |
| `type` | `text`, optional `target` | Focus optionally, then insert text. |
| `screenshot` | none | Return a PNG image. |
| `observe` | optional `includeDom`, `includeScreenshot` | Return structured page context. |
| `getDOM` | none | Return the full DOM tree. |
| `getText` | none | Return visible page text. |
| `evaluate` | `expression` | Evaluate JavaScript. |
| `scroll` | optional `dx`, `dy` | Scroll by CSS pixels. |

All arguments are JSON. `scroll` defaults to `dx: 0` and `dy: 600`.
`includeDom` and `includeScreenshot` default to `false`.

## Observation strategy

Start with `observe`. Its compact result is designed for agent context and
contains page identity, bounded visible text, and accessible controls. Request
`includeDom` only for a task that needs deep structure and
`includeScreenshot` only when pixels are needed. This keeps latency and context
size predictable.

Element references returned by observations include a snapshot revision. Page
or DOM mutations invalidate earlier revisions; observe again after navigation
or a page-changing action.

## Session lifecycle and security

The MCP process owns at most one browser session. Stopping the process closes a
Chrome process that Glass launched. With `--attach`, Glass connects to the
selected existing endpoint but does not claim ownership of its settings.

An MCP client can navigate, execute JavaScript, read page content, and act with
the permissions of the selected browser profile. Use a dedicated profile,
avoid exposing CDP remotely, and review [SECURITY.md](../SECURITY.md) before
granting an AI client access to authenticated pages.
