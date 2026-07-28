# MCP integration

Glass implements an MCP server over standard input and output. The server
starts a browser lazily on the first browser tool call.

Glass currently negotiates MCP protocol version `2024-11-05`. Initialization
with a missing or unsupported version is rejected before a browser starts.
After the initialize response, the client must send
`notifications/initialized`; normal requests are rejected until that
notification arrives. Repeated initialize requests are invalid.

The initialization result also contains a `glass` capability manifest with the
Glass protocol version, supported schema versions, policy-sensitive capability
flags, and platform/browser constraints. See [schema compatibility](schema-compatibility.md)
and the [capability manifest schema](schema/glass-capabilities-v1.schema.json).
Clients that need a specific Glass contract may send `params.glass` with
`protocolVersion` and requested schema versions; incompatible requests fail
before the MCP session becomes ready.

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

For an untrusted MCP client, put policy authority in the fixed client
configuration rather than tool arguments:

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

The hardened preset denies evaluate, screenshots, uploads, downloads,
persistent profiles, attach, and raw CDP by default. Policy failures are MCP
tool errors containing bounded typed JSON with `denied`,
`confirmation_required`, or `invalid_configuration` kinds. One-operation
confirmation tokens are consumed across the shared session and cannot be
supplied through an MCP tool call.

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

Targeting failures return bounded JSON with a typed `kind`, a normalized
`failureKind`, an optional actionability `reason`, and at most eight candidate
summaries. Stale reference failures include a typed `recovery` object pointing
to `reconcileReferences`. These values are derived from the same bounded target
data as observations; raw targets/selectors are never echoed. Any tool call may
opt into an additional bounded failure-trace content item with `includeTrace:
true`.
Evaluated source, typed text, and raw page/CDP errors never cross the MCP error
surface.

The `batch` tool accepts `mode: "fixed"`, `"chain"`, or `"unguarded"`.
Fixed mode requires one `expectedRevision` for every guarded mutation; chain
mode requires the initial revision and carries each successful action's
current revision into the next step. Unguarded mode preserves compatibility.
The `atomic` option remains a separate target pre-resolution check.

The `workflow` tool accepts a declarative `workflow` object, optional typed
`inputs`, and an optional reconciled `checkpoint`. Without a checkpoint it
validates the workflow contract before dispatch; with one it validates the
checkpoint and runs only its safe pending suffix. Results contain step states,
terminal proof, typed outputs, and a deterministic trace. A failed step stops
the workflow; effects observed after dispatch are never treated as safely
reversible.

## Tools

| Tool | Important arguments | Result or effect |
|---|---|---|
| `navigate` | `url`, optional `expectedRevision` | Navigate and return page state. |
| `click` | `target` or `selector`, optional `expectedRevision`, `includeTrace` | Click one element. |
| `clickExpectPopup` | `target` or `selector`, optional `expectedRevision` | Click and return one causally verified popup target. |
| `doubleClick` | `target` or `selector`, optional `expectedRevision` | Double-click one element. |
| `verify` | bounded `predicate`, optional `timeoutMs` | Verify URL, title, visibility, text, topology, dialog, download, revision, or Boolean composition. |
| `hover` | `target` | Move over one element. |
| `drag` | `source`, `destination`, optional `expectedRevision` | Drag between two verified elements. |
| `type` | `text`, optional `target`, `expectedRevision` | Focus optionally, then insert text. |
| `key`, `keyDown`, `keyUp` | `key`, optional `expectedRevision` | Dispatch browser-faithful keyboard events. |
| `shortcut` | `shortcut`, optional `expectedRevision` | Dispatch an explicit modifier shortcut. |
| `clear` | `target`, optional `expectedRevision` | Clear one editable control. |
| `check`, `uncheck` | `target`, optional `expectedRevision` | Verify checkbox/radio state. |
| `select` | `target`, `value`, optional `expectedRevision` | Select one exact option value. |
| `fillForm` | `fields`, optional `expectedRevision` | Fill up to 16 fields and return per-field results. |
| `upload` | `target`, `files`, optional `expectedRevision` | Set 1–16 regular local files. |
| `screenshot` | none | Return a PNG image. |
| `observe` | optional `includeDom`, `includeScreenshot`, `includeFormValues`, `level`, `region` | Return compact page context or a bounded semantic observation. |
| `observeKnowledge` | optional `level`, `freshOnly`, and scope dimensions | Collect fresh semantic evidence and optionally assess local knowledge. |
| `resolveIntent` | `schemaVersion`, `intent`, `action`, optional scope and constraints, `resolutionPolicy` | Resolve one bounded intent and return candidates and evidence without dispatch. |
| `resolveIntentWithKnowledge` | resolve request plus profile/browser scope dimensions | Add eligible historical fingerprint evidence to a fresh intent resolution. |
| `executeIntent` | the resolve request plus `candidateId`, optional `value` | Repeat resolution and dispatch one supported guarded action when policy permits. |
| `knowledgeList` | none | List persistent profile-scoped knowledge without starting Chrome. |
| `knowledgeShow` | `recordId` | Show one validated persistent knowledge record. |
| `knowledgeStats` | none | Report bounded store counts and size. |
| `knowledgeInvalidate` | `recordId`, `state` (`stale`, `contradicted`, or `quarantined`) | Apply an explicit non-eligible lifecycle state. |
| `knowledgePurge` | `origin` | Remove records for one exact origin. |
| `getDOM` | none | Return the full DOM tree. |
| `getText` | none | Return visible page text. |
| `evaluate` | `expression` | Evaluate JavaScript. |
| `scroll` | optional `dx`, `dy`, `expectedRevision` | Scroll by CSS pixels. |
| `batch` | `steps`, optional `mode`, `expectedRevision`, `atomic` | Run a bounded fixed, chain, or unguarded batch. |
| `workflow` | `workflow`, optional `inputs` | Validate and execute a bounded workflow with terminal proof and typed outputs. |
| `wait` | `condition`, optional `timeoutMs`, `includeTrace` | Wait for one typed condition. |
| `diagnostics` | optional `durationMs` | Collect bounded redacted console/network evidence. |
| `acceptDialog`, `dismissDialog` | none | Resolve the currently open JavaScript dialog. |
| `dismissConsent` | none | Dismiss a recognized visible OneTrust/Cookiebot consent control. |
| `download` | `destination`, optional `timeoutMs` | Wait for one scoped download lifecycle. |
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
When `level` is omitted, `observe` retains its compact `PageContext` response.
Set `level` to `summary`, `interactive`, `structured`, `detailed`, or `raw` for
the versioned semantic contract. `region` requires an explicit level and
returns one revision-checked region; semantic levels cannot be combined with
DOM, screenshot, or form-value overlays. See
[semantic observations](semantic-observation.md) for the payload rules.
`resolveIntent` and `executeIntent` use the versioned contract described in
[intent resolution](intent-resolution.md). Execution is revision-checked and
returns separate resolution and action evidence; unsupported intent actions
are rejected before dispatch.
`observeKnowledge` and `resolveIntentWithKnowledge` are explicit opt-ins. Both
collect fresh current-page evidence first. The knowledge variants require the
persistent-profile policy capability; historical fingerprints can only add
`historicalMatch` evidence to a current candidate and cannot authorize an
action. The ordinary `observe` and `resolveIntent` tools do not consult the
store implicitly. See [persistent knowledge](knowledge.md).
`wait.timeoutMs` defaults to 10,000 and must be positive. Cancellation ends an
active wait and scoped network subscriptions are disabled on drop.
Diagnostic and download durations are limited to 30 seconds. Console argument
values, request bodies, sensitive header values/names, URL credentials, and
query values are omitted or redacted. Download destinations must be existing
directories inside the authorized working root.

Target and frame IDs come from the corresponding list tools and are bounded
before Glass retains them. Popup discovery never changes the active target.
Select targets and frames explicitly so a newly opened tab or unrelated frame
cannot become active implicitly.

## Observation strategy

Start with `observe`. Its compact result contains page identity, bounded
visible text, and accessible controls. Request
`includeDom` only for a task that needs deep structure and
`includeScreenshot` only when pixels are needed. This keeps latency and context
size predictable.

Element references returned by observations include a snapshot revision. Page
or DOM mutations invalidate earlier revisions; observe again after navigation
or a page-changing action. For an explicit precondition, pass the observation
revision as `expectedRevision` to any supported mutation. Successful guarded
actions return `status`, the previous and current revisions, an execution ID,
and bounded verification evidence. See [Actions and revisions](actions.md) for
the complete result and failure contract.

## Session lifecycle and security

The MCP process owns at most one browser session. Stopping the process closes a
Chrome process that Glass launched. With `--attach`, Glass connects to the
selected existing endpoint but does not claim ownership of its settings.

An MCP client can navigate, execute JavaScript, read page content, and act with
the permissions of the selected browser profile. Use a dedicated profile,
avoid exposing CDP remotely, and review [SECURITY.md](../SECURITY.md) before
granting access to authenticated pages.

## MCP Registry & Discovery

The [registry submission metadata](mcp-registry.json) and configuration needed
to submit Glass to the MCP Registry are checked in. External
registry publication is a maintainer action and is not implied by this local
checkout.

### Registry Checklist

- **Name:** `glass` — consistent across all registries
- **Description:** "Lightweight local-first browser control plane for Chrome/CDP"
- **Command:** `glass --mcp`
- **Policy recommendation:** `glass --mcp --policy hardened`
- **Repository:** `https://github.com/wanazhar/glass`
- **License:** MIT

### Discoverability

After submission, Glass can be listed alongside other MCP browser tools. The
generic configuration above is suitable for clients that launch local stdio
servers.

If Glass is not yet listed on a registry, file an issue or PR with the
registry's submission process. The metadata above should be sufficient for
any MCP registry that accepts community submissions.
