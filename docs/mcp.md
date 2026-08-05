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
    "workflow": [1],
    "task": [1],
    "webIr": [1]
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

The `task` and `webIr` schemas describe the browser-free Task Protocol and
draft Web IR contracts; their operations do not start Chrome.

The corresponding `taskProtocol` and `webIr` capabilities report
`availableUncertified`: the browser-free contracts are usable, but draft Web IR
and cross-platform runtime evidence are not presented as stable certification.

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
| `preflightNavigation` | Check a normalized URL against navigation policy without starting Chrome or consuming confirmation tokens. |
| `click`, `doubleClick`, `hover` | Perform one verified pointer action. |
| `clickExpectPopup` | Verify one popup caused by the click. |
| `type`, `clear`, `check`, `uncheck`, `select` | Perform one form action. |
| `key`, `keyDown`, `keyUp`, `shortcut` | Send keyboard events. |
| `drag` | Drag between two verified targets. |
| `fillForm` | Fill up to 16 fields. |
| `upload` | Set 1 to 16 regular local files. |
| `screenshot` | Return a PNG image. |
| `observe` | Return compact or semantic page state. |
| `observeBootstrap` | Return bounded advisory URL/title/readiness/text/revision evidence without action targets. |
| `getDOM`, `getText` | Return DOM or visible text. |
| `evaluate` | Run JavaScript when policy permits. |
| `scroll` | Scroll by CSS pixels. |
| `batch` | Run a bounded fixed, chain, or unguarded batch. |
| `compileTask` | Validate a semantic Task Protocol v1 task and return a browser-free execution plan. |
| `validateTask` | Validate a semantic Task Protocol v1 task without compiling or starting Chrome. |
| `inspectWebIr` | Return bounded summary metadata for a validated browser-free Web IR draft. |
| `validateWebIr` | Validate a browser-free Web IR draft without starting Chrome. |
| `diffWebIr` | Return bounded revision-change counts for two validated Web IR drafts. |
| `continuityWebIr` | Classify one entity across two validated Web IR drafts. |
| `wait` | Wait for one typed condition. |
| `diagnostics` | Return bounded redacted console and network data. |
| `acceptDialog`, `dismissDialog` | Resolve the open JavaScript dialog. |
| `dismissConsent` | Dismiss a recognized consent control. |
| `download` | Wait for one scoped download. |
| `listTargets`, `createTarget`, `selectTarget`, `closeTarget` | Manage page targets. |
| `listFrames`, `selectFrame` | Manage frames. |
| `resolveIntent`, `executeIntent` | Resolve and execute an explicit intent. |
| `observeKnowledge`, `resolveIntentWithKnowledge` | Use the optional local knowledge store. |
| `inspectPage`, `findTarget` | Return bounded semantic page state and candidates without acting. |
| `actAndVerify` | Execute one explicit intent with optional postcondition evidence. |
| `extractStructured` | Return bounded typed fields with revision provenance. |
| `recoverRun` | Return conservative recovery guidance after indeterminate execution. |
| `sessionSnapshot` | Create, list, inspect, diff, or purge redacted local snapshots. |
| `knowledgeList`, `knowledgeShow`, `knowledgeStats` | Read knowledge records. |
| `knowledgeInvalidate`, `knowledgePurge` | Change knowledge lifecycle state. |
| `lease/acquire`, `lease/renew`, `lease/release` | Manage daemon mutation leases. |

`extractStructured` fields are bounded semantic projections. Each field has a
`name`, a semantic `path`, and a `kind`. Supported explicit kinds are
`string`, `optionalString`, `number`, `currency`, `date`, `dateTime`,
`boolean`, `url`, `enum`, `list`, `record`, `table`, and `repeatedItems`;
legacy `scalar`, `optionalScalar`, and `object` kinds remain accepted.
Successful results include the source revision and route, compatibility
`provenance` paths, field-level `fieldProvenance` with optional region/entity
references, and `limits` containing requested bounds, observed item count,
serialized bytes, and truncation status. Array fields can be resumed with a
bounded `startIndex`; when more items remain, the result includes a
revision-bound `continuation` containing the next index, source revision, and
source route. Callers should pass that returned continuation object in the
next request; Glass rejects it if the fresh semantic observation has a
different revision or route. Table and repeated-collection fields also return
bounded `recordItems`; each item carries its source field, zero-based source
index, semantic value, and any evidence-backed entity references.
The compatibility `records` envelope remains unchanged. Extraction is read-only
and never serializes authored input values.
Semantic table and collection regions also expose bounded `structuredRecords`
under the region projection. Table records use bounded column-header keys;
collection records expose semantic name, heading, description, and link fields
when those accessibility facts are present. Browser-backed task extraction
uses this projection and falls back to bounded interactive targets when the
page exposes no structured accessibility records.

The `diagnostics` result also includes bounded `startupDiagnostics` timings
(`launch_endpoint_ms`, `page_target_wait_ms`, `cdp_connect_ms`,
`target_attach_ms`, `event_setup_ms`, `policy_arm_ms`, and `total_startup_ms`)
and `lifecycle` milestones (`browserReady`, `navigationStarted`, `evidenceReady`,
and `actionVerified`). These are metadata-only and never include URLs, target
IDs, page text, or credentials. MCP initialization/transport negotiation means
only that the server is ready to answer protocol requests; it is not evidence
that Chrome or the selected page is browser-ready. Use the diagnostic
milestones when a caller needs to distinguish process/session readiness from
page evidence or a verified action.

### Preflight, confirm, then navigate

Call `preflightNavigation` before `navigate` when a caller needs a
browser-free policy decision. The response contains the normalized URL, host,
decision, reason, and `confirmationRequired`. Preflight is read-only: it does
not start Chrome, resolve DNS, or spend a `--policy-confirm-once` token.

If the decision is `allow`, call `navigate` with the same URL. If a future
policy reports `confirmation_required`, obtain the caller's explicit
confirmation through the configured policy boundary and then call `navigate`;
preflight itself never authorizes or consumes that confirmation. A `deny`
decision must not be bypassed by calling `navigate`. In hardened mode, an
unpinned allowed host is reported as denied with a host-pinning reason rather
than being resolved or silently allowed.

### Navigation result metadata

Revision-aware navigation returns an additive `identity` object alongside the
existing `page` and revision fields. It contains the normalized requested URL,
the observed final URL after lifecycle completion, a parsed `sameOrigin` result,
advisory page-state `classification`, and bounded redirect evidence. The
`redirectCount` is `null` when the bounded CDP/network evidence needed to
derive a count was not available; it is never inferred from a URL difference.
Authority credentials are removed from serialized URLs.

### Validate a task without starting Chrome

`validateTask` performs strict Task Protocol validation and returns only bounded
task metadata. It does not compile a plan, start Chrome, acquire a mutation
lease, resolve targets, or execute browser actions. Input values are consumed
only during validation and are never included in the response.

The canonical Glass operation is `task.validate`; MCP retains JSON-RPC framing
and the `validateTask` tool name.

The MCP dispatch path constructs the canonical request and invokes the typed
protocol helper; MCP-only response options are not part of the task payload.

The successful MCP content item contains JSON shaped like:

```json
{
  "valid": true,
  "schemaVersion": 1,
  "task": "form.fill"
}
```

For a valid JSON task that fails Task Protocol validation, the tool returns
`isError: true` with a bounded `taskValidation` error containing `path` and
`reason`.

For example, an invalid `form.fill` with no bounded inputs returns:

```json
{
  "kind": "taskValidation",
  "path": "inputs",
  "reason": "form.fill requires at least one bounded input"
}
```

### Inspect and validate a Web IR draft without starting Chrome

`inspectWebIr` and `validateWebIr` consume a bounded draft JSON object and never
start Chrome, acquire a mutation lease, or dispatch browser actions. Both tools
validate graph invariants first. `validateWebIr` returns only schema and revision
metadata:

```json
{
  "valid": true,
  "schemaVersion": 1,
  "revision": 7
}
```

`inspectWebIr` returns bounded summary metadata without exposing entity IDs,
relationships, or page content:

```json
{
  "schemaVersion": 1,
  "revision": 7,
  "entityCount": 2,
  "relationshipCount": 1,
  "coverage": {
    "structural": "strong",
    "semantic": "strong",
    "interactiveEntitiesObserved": 1,
    "opaqueRegions": 0,
    "reasons": []
  },
  "truncated": false,
  "opaqueRegions": 0,
  "diagnosticCount": 0,
  "relationshipHintDiagnosticCount": 0
}
```

Invalid drafts return `isError: true` with a bounded `webIrValidation` object
containing `path` and `reason`. These tools are intended for offline inspection
and protocol conformance; they do not turn a draft into browser authority.

The server dispatches these tools through the typed canonical `webIr.*`
protocol operations; MCP-only response options are not forwarded into drafts.

`diffWebIr` validates both drafts and returns only bounded change counts and
revision metadata:

```json
{
  "schemaVersion": 1,
  "fromRevision": 7,
  "toRevision": 8,
  "entityAddedCount": 0,
  "entityRemovedCount": 0,
  "entityChangedCount": 1,
  "relationshipAddedCount": 0,
  "relationshipRemovedCount": 0,
  "coverageChanged": false,
  "limitsChanged": false,
  "diagnosticsChanged": false,
  "relationshipHintDiagnosticsChanged": false
}
```

`continuityWebIr` validates both drafts and classifies one source entity as
`unchanged`, `changed`, `rebound`, `removed`, or `ambiguous`. It returns the
revision-local `currentId` only when a unique current entity is identified.
Both tools are browser-free, require no mutation lease, and omit entity
payloads, relationships, and page content from successful responses.

### Compile a task without starting Chrome

`compileTask` validates authored intent and returns a typed plan. It does not
start Chrome, acquire a mutation lease, resolve targets, or execute browser
actions. Input values are consumed only during validation and are not included
in the returned plan.

The MCP dispatch path constructs the canonical request and invokes the typed
protocol helper; MCP-only response options are not part of the task payload.

The canonical Glass operation is `task.compile`; MCP retains JSON-RPC framing
and the `compileTask` tool name.

Example request:

```json
{
  "jsonrpc": "2.0",
  "id": "compile-1",
  "method": "tools/call",
  "params": {
    "name": "compileTask",
    "arguments": {
      "task": {
        "schemaVersion": 1,
        "task": "form.fill",
        "scope": {
          "regionName": "Shipping address",
          "entityKind": "form"
        },
        "inputs": {
          "city": "Kuching"
        },
        "limits": {
          "maxActions": 16,
          "timeoutMs": 15000,
          "maxItems": 128
        },
        "risk": "localMutation",
        "ambiguity": "fail",
        "revision": "exact",
        "postconditions": [
          {"kind": "validationClear"}
        ]
      }
    }
  }
}
```

The successful MCP content item contains JSON shaped like:

```json
{
  "plan": {
    "schemaVersion": 1,
    "taskSchemaVersion": 1,
    "task": "form.fill",
    "scope": {"regionName": "Shipping address", "entityKind": "form"},
    "limits": {"maxActions": 16, "timeoutMs": 15000, "maxItems": 128},
    "ambiguity": "fail",
    "revision": "exact",
    "risk": "localMutation",
    "confirmationRequired": false,
    "steps": [
      {"ordinal": 1, "operation": "observeScope"},
      {"ordinal": 2, "operation": "fillInputs", "inputNames": ["city"]}
    ],
    "postconditions": [{"kind": "validationClear"}]
  }
}
```

The plan is not an action result. A later execution layer must reconcile the
plan against a current Web IR revision and apply its policy and confirmation
gates.

Keep sensitive input values in the local client boundary and send them only
when the task requires them. Glass does not serialize those values into the
compiled plan or its canonical response; clients should still avoid retaining
raw request bodies longer than necessary.

For a valid JSON task that fails Task Protocol validation, the tool returns
`isError: true` without starting Chrome. The first text content item is a
bounded JSON error:

```json
{
  "kind": "taskCompilation",
  "path": "inputs",
  "reason": "form.fill requires at least one bounded input"
```

The `path` identifies the authored field that must be corrected; `reason` is
safe to display to the caller. Structural argument errors, such as a missing
`task` object, remain ordinary MCP tool errors.

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

`observeBootstrap` is a bounded, page-state-only readiness sample. It returns
URL, title, ready state, bounded visible text, revision, consistency, and
completeness metadata; it never resolves or returns action-authorizing targets.
Use `observe` for authoritative target resolution before any action.

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
