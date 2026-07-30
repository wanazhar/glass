# MCP integration

Glass runs an MCP server over standard input and standard output. It starts
Chrome when the first browser tool needs it.

Glass negotiates MCP protocol version `2024-11-05`. The client must send
`notifications/initialized` after `initialize`. Glass rejects normal
requests before that notification. A second `initialize` request is invalid.

The initialization result contains a discovery manifest in `glass` and an
immutable effective agreement in `glassAgreement`. The manifest reports the
complete protocol version, schema inventory, policy-sensitive capabilities,
explicit capability statuses, and platform and browser constraints. The
agreement reports the exact selected schema version and negotiated capability
status for this connection. Clients should use `capabilityStatuses` when
present; the legacy boolean map remains for compatibility.

## Configure a client

Build or install Glass. The deterministic command below uses the actual
installed executable path:

```console
glass mcp-config --client generic
glass mcp-config --client claude-code
glass mcp-config --client codex
```

The generated configuration starts the binary with `--mcp`. Keep stdout
reserved for protocol messages; Glass writes diagnostics to stderr.

You may add session options:

```json
{
  "command": "/absolute/path/to/glass",
  "args": ["--incognito", "--interaction", "fast", "--mcp"]
}
```

For untrusted input, set the policy in fixed configuration:

```json
{
  "command": "/absolute/path/to/glass",
  "args": [
    "--policy", "hardened",
    "--incognito",
    "--policy-allow-host", "example.com",
    "--mcp"
  ]
}
```

Use an absolute binary path in a graphical client. Keep stdout reserved for MCP
messages. Glass writes diagnostics to stderr.

## Negotiation

A client may request Glass versions in `initialize.params.glass`:

```json
{
  "protocolVersions": [1],
  "schemas": {
    "action": [1],
    "workflow": [1]
  },
  "requires": ["workflowResume"],
  "optional": ["extensions"],
  "acceptsExperimental": false
}
```

Glass returns the selected versions and effective capability statuses in
`glassAgreement`. Required capabilities fail before the session becomes ready;
unknown optional capabilities are omitted. Experimental capabilities require
explicit acceptance. Glass rejects unknown schemas, empty version lists, and
requests with no common supported version.

Read [schema compatibility](schema-compatibility.md) for version rules.

## Transport limits

Glass accepts newline-delimited JSON-RPC and `Content-Length` frames.

| Resource | Limit |
|---|---:|
| Header or newline request | 8 KiB |
| Request body | 4 MiB |
| Serialized response | 32 MiB |
| Active requests | 8 |
| Queued responses | 16 |

Glass rejects malformed and oversized input without allocating the declared
body. It replaces an oversized result with a small JSON-RPC error.

The frame deadline starts when the first byte arrives. An idle server may wait
without treating the connection as a partial frame.

## Cancellation

Cancel a request with `notifications/cancelled` and the original request ID.
Glass returns error code `-32800`.

Cancellation drops local wait and pending CDP response state. It cannot undo
browser input or JavaScript that Chrome accepted.

Glass handles requests concurrently. It serializes browser operations through
one session. Requests above the active limit receive an overload error.

## Errors and privacy

Targeting errors contain a bounded kind, failure kind, optional reason, and at
most eight candidate summaries. Stale references include recovery data.

Glass does not echo raw selectors, evaluated source, typed text, or raw page and
CDP errors in MCP errors.

A tool may request one bounded failure-trace content item with
`includeTrace: true`.

## Main tools

| Tool | Result |
|---|---|
| `navigate` | Navigate and return page state. |
| `click`, `doubleClick`, `hover` | Perform one verified pointer action. |
| `clickExpectPopup` | Verify one popup caused by the click. |
| `type`, `clear`, `check`, `uncheck`, `select` | Perform one form action. |
| `key`, `keyDown`, `keyUp`, `shortcut` | Send keyboard events. |
| `drag` | Drag between two verified targets. |
| `fillForm` | Fill up to 16 fields. |
| `upload` | Set 1 to 16 regular local files. |
| `screenshot` | Return a PNG image. |
| `observe` | Return compact or semantic page state. |
| `getDOM`, `getText` | Return DOM or visible text. |
| `evaluate` | Run JavaScript when policy permits. |
| `scroll` | Scroll by CSS pixels. |
| `batch` | Run a bounded fixed, chain, or unguarded batch. |
| `workflow` | Validate and run a bounded workflow. |
| `wait` | Wait for one typed condition. |
| `diagnostics` | Return bounded redacted console and network data. |
| `acceptDialog`, `dismissDialog` | Resolve the open JavaScript dialog. |
| `dismissConsent` | Dismiss a recognized consent control. |
| `download` | Wait for one scoped download. |
| `listTargets`, `createTarget`, `selectTarget`, `closeTarget` | Manage page targets. |
| `listFrames`, `selectFrame` | Manage frames. |
| `resolveIntent`, `executeIntent` | Resolve and execute an explicit intent. |
| `observeKnowledge`, `resolveIntentWithKnowledge` | Use the optional local knowledge store. |
| `knowledgeList`, `knowledgeShow`, `knowledgeStats` | Read knowledge records. |
| `knowledgeInvalidate`, `knowledgePurge` | Change knowledge lifecycle state. |
| `lease/acquire`, `lease/renew`, `lease/release` | Manage daemon mutation leases. |

All arguments use JSON. Targets use the same locator forms as the CLI.
`includeDom` and `includeScreenshot` default to false. A semantic
`level` is one of `summary`, `interactive`, `structured`,
`detailed`, or `raw`.

## Safety rules

The `batch` tool supports `fixed`, `chain`, and `unguarded` modes.
`fixed` requires a revision for each guarded mutation. `chain` carries the
current revision from one successful action to the next.

The `workflow` tool validates before dispatch. A checkpoint runs only its
reconciled safe suffix. A failed post-dispatch effect is not treated as
reversible.

The `observe` tool keeps DOM, screenshot, and form values explicit. A level
or region cannot silently add these fields.

Target and frame IDs come from list tools. Popup discovery does not select a
target. Select targets and frames explicitly.

## Session security

One MCP process owns at most one browser session. Stopping the process closes a
Chrome process that Glass started. With `--attach`, Glass connects to the
selected endpoint and does not own its settings.

An MCP client can navigate, run JavaScript, read page content, and use the
selected browser profile. Use a dedicated profile. Keep CDP local. Read
[SECURITY.md](../SECURITY.md) before connecting an untrusted client.

## Registry metadata

The local registry metadata is in `mcp-registry.json`. It is submission
data. It does not mean that Glass is listed in an external registry.

The registry entry uses:

- name: `glass`;
- command: `glass --mcp`;
- license: MIT; and
- repository: `https://github.com/wanazhar/glass`.

External registry submission is a maintainer action.
