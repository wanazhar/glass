# Private development cockpit and mobile presentation

Status: Current 0.3.13 source behavior (private cockpit API); the former card
and capsule design is historical and is not an implementation contract.

This document covers the Glass Dev private cockpit and how a remote/mobile
operator may present the same development workspace. It does **not** define a
second phone TUI. The current Glass Dev TUI is one geometry-responsive reducer
with Desktop, Compact, and Phone classes; see
[Development TUI](development-tui.md). The standalone browser-only TUI is a
separate product and adapter; see [Standalone Browser TUI](tui.md).

## Boundaries and ownership

```text
Glass Dev TUI / local browser client / SSH-forwarded browser
                              |
                              v
                 LocalCockpit (127.0.0.1:ephemeral)
                 token URL + one cockpit thread
                              |
                              v
                 SharedDevelopmentWorkspace
       trust · generation/revision · agents · tasks · Git · GitHub
```

`LocalCockpit` binds only to IPv4 loopback on an OS-selected ephemeral port,
generates a random URL-safe token, and serves a small private HTML view plus
JSON endpoints. It receives the same `SharedDevelopmentWorkspace` handle as
Glass Dev. It does not own Chrome pixels, a browser session, editor buffers,
PTYs, a resident agent loop, or a second task authority. The TUI's
`SnapshotWorker` remains the owner of Glass Dev background refresh/tool/screenshot
jobs; cockpit requests are handled by the private cockpit thread and execute
through the governed workspace tool path.

This is intentionally not a public relay or image transport. For a remote
operator, forward the loopback port with SSH (or the operator's existing
approved transport); do not expose the listener directly. Browser Remote View
is a distinct browser presentation subsystem and must not be conflated with
this workspace-state cockpit.

## Current responsive presentation

The HTML view is intentionally small and responsive to the browser viewport. It
presents loopback-only workspace state rather than the historical six-view
phone dock. The current Rust TUI owns the authoritative terminal layouts:

```text
Auto Glass Dev layout
  width < 72 or height < 22  -> Phone (single pane)
  width < 118 or height < 32 -> Compact (navigation + surface)
  otherwise                  -> Desktop (navigation + surface + context)

Phone TUI
┌──────────────────────────────┐
│ header · trust/mode · status │ 2 rows
├──────────────────────────────┤
│ one active Agent/Code/App/   │ min 5 rows
│ Tasks/More surface           │
├──────────────────────────────┤
│ status, or composer + status │ 2 / 3 rows
└──────────────────────────────┘
```

Phone exposes five direct destinations: Agent, Code, App, Tasks, and More.
Desktop and Compact expose Agent, Code, App, Terminal, Tasks, Git, Debug, and
More; Compact removes the context column. Phone changes geometry only: the
same shared workspace, revision checks, agent ownership, and worker boundaries
apply. Desktop/Compact/Phone details, modal precedence, editor projection,
and key routing are authoritative in [Development TUI](development-tui.md),
not duplicated here.

The private HTML cockpit currently displays a refreshable state document and a
GitHub review action. Its state schema contains:

| Field | Ownership and meaning |
|---|---|
| `schemaVersion` | development cockpit schema identifier |
| `root` | canonical project root display |
| `generation` / `projectRevision` | workspace and project optimistic-concurrency guards |
| `trust` | current workspace trust label |
| `agents` | bounded resident-agent snapshot or an embedded error |
| `tasks` | bounded task snapshots or an embedded error |
| `git` | Git status, `null` outside a repository, or an embedded error |
| `github` | cached GitHub origin/auth/review status |

No browser screenshot, prompt text, source buffer, secret-bearing URL, token,
PTY output, or arbitrary raw workspace payload is added to this state contract.

## HTTP contract and lifecycle

After `cockpit start` (from the Glass Dev command palette), the state owns a
URL shaped like `http://127.0.0.1:PORT/TOKEN/`. The token is required as the
first path segment; requests without it receive `401`. Supported routes are:

| Method and route | Behavior |
|---|---|
| `GET /TOKEN/` | serve the private HTML view |
| `GET /TOKEN/v1/health` | return `{"ok":true}` |
| `GET /TOKEN/v1/state` | return the bounded workspace state above |
| `POST /TOKEN/v1/command` | execute one governed tool request |

`LocalCockpit` accepts at most 128 KiB per request and emits at most 512 KiB per
response. The connection read timeout is two seconds. Unknown routes return
`404`; malformed/invalid requests return `400`; workspace conflicts return
`409`; state/tool or oversized-response failures return `503`. Responses are
`no-store` and the connection closes after each response.

A command request has `name`, optional `arguments`, optional `allowMutation`,
optional `confirmed`, and optional `expectedGeneration` and
`expectedProjectRevision`. Names are non-empty, at most 128 bytes, and limited
to ASCII letters/digits plus `.`, `-`, and `_`. The effective mutation
permission is `allowMutation && confirmed`; the request actor is the external
`cockpit` actor. Omitted expected generation/revision values are filled from
the current workspace, while supplied stale values fail closed through the
normal development tool contract. The browser client must not treat a `200`
response as permission to bypass trust or revision checks.

```text
GET state  ──> read shared workspace ──> bounded JSON
POST command ──> parse + bounds
              ──> expected generation/revision
              ──> governed execute_tool
              └─> result or 400/409/503
```

Starting an already-running private cockpit returns its existing local URL.
Stopping it drops the listener and joins its thread; dropping the Glass Dev
state also shuts it down. `cockpit status` reports `running · URL` or
`not running · use cockpit start`. Project identity and state remain in the
shared workspace when a browser disconnects or the cockpit is stopped.

## Remote/mobile security and recovery rules

- Keep the tokenized URL private; loopback binding is a required boundary, not
  an optional deployment mode.
- Use SSH port forwarding for remote access. This cockpit has no public relay,
  account/session service, or mobile-specific authority path.
- Treat `generation` and `projectRevision` as optimistic concurrency values.
  On `409`, reload state and rebuild the action rather than retrying a stale
  mutation blindly.
- A browser failure does not destroy workspace state. Browser recovery and
  semantic-only operation belong to the embedded Browser workspace in the App
  surface; visual pixels remain optional and bounded.
- Trust, confirmation, and actor policy remain enforced by the development
  tool executor even when a command originates from the cockpit.

## Historical design evidence (not current behavior)

The following records the former 0.3.3 cockpit proposal for traceability only.
It is immutable design history, not a command, layout, worker, or security
contract for the checked-in implementation.

### Historical resident session and capsule model

```text
MCP/TUI/client
     │ attach canonical project root
     v
ResidentDevelopmentSessions ── bounded LRU / idle expiry
     │ owns
     ├── ProjectWorkspace (buffers, revisions, actors, timeline)
     ├── managed PTY jobs and bounded output
     └── reconnect capsule (non-sensitive preferences/cursors only)
             │
             └── BrowserConnectionController
```

The proposal expected a resident session to survive browser recovery and a
non-sensitive capsule to retain view/scroll/live preferences. It explicitly
excluded prompts, command input, pixels, cookies, temporary ports/tokens,
browser PIDs, and stale target IDs.

### Historical phone card layout

```text
┌─ glass / checkout ─ SSH · semantic ┐
│ agent RUNNING · app ATTACHED · r83 │
├─ NEEDS YOU (1) ────────────────────┤
│ ! browser port conflict      Enter │
├─ AGENT ────────────────────────────┤
│ observe → patch → HMR → verify     │
├─ LIVE APP ─────────────────────────┤
│ /checkout · rev 83 · fresh         │
├─ UNDERSTANDING ────────────────────┤
│ checkout.form · submit enabled     │
├─ TESTS / PROCESS ──────────────────┤
│ ✓ unit 81/81 · ● dev :3000         │
├────────────────────────────────────┤
│ 1 Overview 2 Agent 3 Browser       │
│ 4 Diff 5 Project 6 Process   : cmd │
└────────────────────────────────────┘
```

The historical Overview was an adaptive stack of needs-attention, live-app,
agent, understanding, and process cards. Short terminals collapsed to a
two/three-card priority window with PageUp/PageDown paging. A sticky project
header, composer/navigation, and help/status footer remained visible. Browser
pixels were only a preview and could not displace agent or process state.

### Historical views and interaction table

```text
Overview ─ summary cards and attention
Agent    ─ current step, tools, approvals and reconciliation
Browser  ─ semantic page, compact preview, recovery and Remote View
Diff     ─ changed files and verification evidence
Project  ─ files, editor buffer and diagnostics
Process  ─ PTYs, tests, logs and lifecycle controls
```

The proposal gave `1`–`6` view selection, Tab cycling, `:` action sheets, `?`
context help, `inbox`, `notify on|off|status`, `tap` semantic actions,
`verify card`, `capsule save|show|clear`, and `live quality auto`. It also
specified `browser ...` probe/launch/attach/target/reconnect/disconnect and
`browser remote-view open`, plus Y/Enter and N/Esc for a 120-second mutation
sheet. None of these are current private cockpit routes.

### Historical render and visual rules

The proposal decoupled reducer and renderer, coalesced bursts, and capped local
presentation at 60 cell frames/s, measured remotes at 30, and constrained or
unknown/Mosh transports at 20. It dropped key releases and noisy mouse
move/release/drag events, bounded bracketed paste, and retained latest browser
frames under backpressure. Kitty frames carried geometry/generation identity and
were not retransmitted for status-only redraws. It suspended capture for hidden
Browser views and materialized only a newest PNG plus an ANSI thumbnail in
Overview.

Its semantic overlay bound actions to browser and geometry revisions:

```text
┌─ Semantic actions · revision 83 ───┐
│ [1] Open menu                      │
│ [2] Search                         │
│ [3] Add to cart                    │
│ [4] Continue checkout              │
└────────────────────────────────────┘
```

Stale targets failed closed and requested fresh observation. The historical
browser recovery sheet offered automatic-port launch, inspect/attach, explicit
port choice, or semantic-only mode while retaining project/agent/process
state. Its attention order was blocking failure/confirmation, current agent
action, app health, semantic freshness, process/tests, diff, then telemetry;
notification bodies excluded prompts, output, source, secrets, frames, and
tokens.

## Source of truth

The private cockpit implementation is
[`development/cockpit.rs`](../../crates/glass-dev/src/development/cockpit.rs),
and its TUI integration is in
[`tui/state.rs`](../../crates/glass-dev/src/tui/state.rs) and
[`tui/render.rs`](../../crates/glass-dev/src/tui/render.rs). The shared browser
connection/presentation contract is
[`browser_workspace/mod.rs`](../../crates/glass-browser/src/browser_workspace/mod.rs).
