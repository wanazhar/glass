# Browser connection controller and Remote View v1

Status: Accepted for 0.3.3

## Ownership

The workspace owns one long-lived `BrowserConnectionController`. The
controller owns at most one authoritative `BrowserSession` generation.
Project files, editor buffers, PTYs, tests, agent conversation and development
timeline do not belong to that generation and survive its failure.

```text
Glass workspace
├─ ProjectWorkspace (survives)
├─ agent/process/editor state (survives)
└─ BrowserConnectionController
   ├─ desired configuration (persistent preference)
   ├─ endpoint classification / targets (ephemeral)
   ├─ session generation (ephemeral authority)
   ├─ terminal latest-frame presentation
   └─ Remote View v1 (optional view of this generation)
```

## Controller state

```text
Idle -> Probing -> Launching/Attaching -> Connected -> SelectingTarget
                  |                         |
                  v                         v
              PortConflict             Disconnected
                  |                         |
                  └--------> Recovering <---┘
                                |
                         Connected / Failed / SemanticOnly
```

Each state includes a monotonic generation and bounded reason. Connected state
includes preferred/effective endpoint, ownership, selected target identity and
fresh semantic revision. Disconnected/recovering state has no authoritative
current page revision.

## Endpoint classification

| Classification | Proof | Default recovery |
|---|---|---|
| compatible DevTools | bounded `/json/version` and page-target metadata validate | offer attach/target picker/new owned browser |
| recoverable Glass-owned | current workspace/session evidence matches | reconnect |
| unrelated listener | TCP listener exists; DevTools metadata is incompatible | launch on automatic endpoint |
| unknown | insufficient/timeout/malformed evidence | fail closed; automatic endpoint remains available |

Automatic launch asks the OS for a loopback port and holds the reservation
until immediately before Chrome launch. Because Chrome cannot inherit that
listener, a bind race remains possible; Glass handles it with two fresh,
bounded OS-selected retries before returning to the visible recovery sheet.
There is no unbounded sequential port scan.
| free | bind/probe establishes availability | launch if explicitly configured; prefer browser-assigned endpoint |

A successful TCP connection is never sufficient authority to attach. Discovery
is bounded by time, bytes, target count and privacy-aware projection.

## Recovery layout

Desktop/compact overlay:

```text
┌─ Browser needs attention ─────────────────────────────────────┐
│ Preferred :9222 · unrelated listener                         │
│ Browser disconnected · project / agent / processes running   │
│                                                              │
│ > Launch on automatic port (recommended)                     │
│   Inspect / attach endpoint     Select target                │
│   Choose port                   Retry                        │
│   Continue semantic-only                                     │
│                                                              │
│ Enter apply   ↑↓ select   Esc close                          │
└──────────────────────────────────────────────────────────────┘
```

Phone decision sheet:

```text
┌─ NEEDS YOU ───────────────────────┐
│ BROWSER · port 9222 busy          │
│ unrelated listener               │
│ project + agent still running     │
│                                  │
│ > Launch automatically           │
│   Inspect / attach               │
│   Choose port                    │
│   Semantic only                  │
│                                  │
│ Enter choose · Esc later         │
└──────────────────────────────────┘
```

The target picker shows at most a bounded number of page targets. It displays
title, redacted origin/path, type, selected/associated state and never persists
full sensitive URLs.

## Commands (design-era pseudocode)

The command grammar below is retained as accepted 0.3.3 design-era pseudocode,
not current operator syntax. Current development-TUI operators use
`:browser start` for browser startup; automatic port selection is offered by
the in-TUI recovery sheet rather than a `browser launch --port auto` command.
For current Remote View operations, use `:browser remote-open`,
`:browser remote-status`, and `:browser remote-revoke`. Standalone
`glass-browser` does not provide Remote View.

`:` opens a filtered palette. Printable routes remain usable on mobile:
```text
:browser status
:browser connect
:browser launch [--port auto|PORT] [--headed|--headless]
                [--profile NAME|--incognito] [--chrome-path auto|PATH]
:browser attach [--port PORT] [TARGET]
:browser targets [PORT]
:browser target [ID]
:browser reconnect
:browser disconnect
:browser semantic-only
:browser remote-view open|status|close
```

Profile, incognito, headed/headless, browser executable and target policy are
editable through controller commands/overlays. Intentional preference may be
stored; selected ephemeral port, PID, temporary token and stale target ID may
not become permanent configuration.

## Agent and semantic freshness

Detached/recovering state disables live `@page`, `@browser`, `@selection` and
browser mutation tools. Historical Web IR/memory may be labeled historical but
cannot satisfy current-page checks. Successful connection/target selection
performs fresh extraction before publishing a current revision and refreshing
agent capabilities. Mutation still requires actor authority, lease, policy,
revision and confirmation.

## Remote View v1

Remote View is an explicitly opened loopback service for the current session:

```text
BrowserSession generation N
  ├─ terminal mailbox
  └─ Remote View service on 127.0.0.1:ephemeral
       ├─ scoped random token (memory only)
       ├─ HTML control page
       ├─ latest-frame-only WebSocket
       └─ revisioned pointer / keyboard messages
```

The service never launches or navigates another browser. It binds only
`127.0.0.1`/`::1`, exposes an SSH-forward command, stores no frames, limits
connections/message/frame sizes, refuses missing/expired/revoked tokens, and
stops before the session generation closes. Pointer input supplies browser,
geometry and frame revisions and fails closed when stale. Incognito/private
frames never enter memory, logs or persistence.

## State variants

- **Loading/probing**: project shell remains interactive; browser card shows
  exact current step.
- **Port conflict**: recovery overlay/sheet opens and the attention inbox gains
  one deduplicated item.
- **Selecting target**: picker owns focus; `Esc` returns to recovery without
  ending the workspace.
- **Disconnected**: terminal/Remote View frames are cleared and semantic
  freshness is unavailable.
- **Semantic-only**: intentional, non-error state with reconnect actions.
- **Remote View active**: status includes local endpoint, expiry and revoke;
  the token is shown only at creation/explicit reveal.

## Tests

- Endpoint fixture tests cover compatible, unrelated, malformed, timeout and
  free endpoints on arbitrary configured ports.
- Reducer/snapshot tests cover desktop and phone recovery plus target picker.
- Integration tests prove project/process identity survives generation changes
  and a fresh semantic revision follows reconnect.
- Remote View tests prove loopback binding, token scope/revocation, latest-frame
  replacement, same identity and stale input rejection.
