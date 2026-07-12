# MCP integration

Glass implements an MCP server over standard input and output. The server
starts a browser lazily on the first browser tool call.

Glass currently negotiates MCP protocol version `2024-11-05`. Initialization
with a missing or unsupported version is rejected before a browser starts.
After the initialize response, the client must send
`notifications/initialized`; normal requests are rejected until that
notification arrives. Repeated initialize requests are invalid.

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

## Transport limits and cancellation

Glass accepts newline-delimited JSON-RPC and `Content-Length` framing. Frames
must be valid UTF-8 and use a strict decimal content length followed by one
blank line. The transport bounds are:

| Resource | Limit |
|---|---:|
| Header or newline request | 8 KiB |
| Content-length request body | 4 MiB |
| Serialized response | 32 MiB |
| Accepted concurrent requests | 8 |
| Queued responses | 16 |

Oversized or malformed input is rejected without allocating the declared body.
An oversized result is replaced by a small JSON-RPC error instead of being
partially written. The frame deadline begins when the first byte arrives, so an
idle stdio server can wait indefinitely without treating inactivity as a
partial-frame timeout.

Clients cancel an in-flight request with `notifications/cancelled` and the
original `requestId`. Glass returns error code `-32800` for that request and
keeps the browser session available. Cancellation drops local waiting and
pending CDP response state; it cannot undo browser input or JavaScript already
accepted by Chrome. Cancellation reasons are neither logged nor returned.

Glass accepts requests concurrently so cancellation remains responsive, while
browser operations are serialized through the single owned session. Requests
beyond the active limit receive a bounded overload error.

Targeting failures return bounded JSON with a typed `kind`, an optional
actionability `reason`, and at most eight candidate summaries. These values are
derived from the same bounded agent-facing target data as observations; raw
targets/selectors are never echoed. Other tool failures return generic
`browser tool failed` text. Evaluated source, typed text, and raw page/CDP
errors never cross the MCP error surface. Local tracing remains metadata-only.

## Tools

| Tool | Important arguments | Result or effect |
|---|---|---|
| `navigate` | `url` | Navigate and return page state. |
| `click` | `target` or `selector` | Click one element. |
| `doubleClick` | `target` or `selector` | Double-click one element. |
| `hover` | `target` | Move over one element. |
| `drag` | `source`, `destination` | Drag between two verified elements. |
| `type` | `text`, optional `target` | Focus optionally, then insert text. |
| `key`, `keyDown`, `keyUp` | `key` | Dispatch browser-faithful keyboard events. |
| `shortcut` | `shortcut` | Dispatch an explicit modifier shortcut. |
| `clear` | `target` | Clear one editable control. |
| `check`, `uncheck` | `target` | Verify checkbox/radio state. |
| `select` | `target`, `value` | Select one exact option value. |
| `upload` | `target`, `files` | Set 1–16 regular local files. |
| `screenshot` | none | Return a PNG image. |
| `observe` | optional `includeDom`, `includeScreenshot` | Return structured page context. |
| `getDOM` | none | Return the full DOM tree. |
| `getText` | none | Return visible page text. |
| `evaluate` | `expression` | Evaluate JavaScript. |
| `scroll` | optional `dx`, `dy` | Scroll by CSS pixels. |
| `wait` | `condition`, optional `timeoutMs` | Wait for one typed condition. |
| `listTargets` | none | List bounded page targets without selecting one. |
| `createTarget` | `url` | Create a page target without selecting it. |
| `selectTarget` | `id` | Select the target for later tools. |
| `closeTarget` | `id` | Close a target; no implicit replacement is selected. |
| `listFrames` | none | List bounded frames in the active target. |
| `selectFrame` | `id` | Select the frame execution context for later tools. |

All arguments are JSON. `scroll` defaults to `dx: 0` and `dy: 600`.
`target` uses the same explicit locator forms as the CLI: `ref=`, `name=`,
`role=...;name=...`, `text=`, `css=`, and `ordinal=`. Bare references and exact
accessible names remain compatible. The legacy `selector` argument is treated
as CSS, but must still resolve uniquely.
`includeDom` and `includeScreenshot` default to `false`.
`wait.timeoutMs` defaults to 10,000 and must be positive. Cancellation ends an
active wait and scoped network subscriptions are disabled on drop.

Target and frame IDs come from the corresponding list tools and are bounded
before Glass retains them. Popup discovery never changes the active target.
Select targets and frames explicitly so an agent cannot drift into a newly
opened tab or unrelated frame.

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
