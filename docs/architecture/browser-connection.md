# Browser connection controller and Remote View

Status: Current 0.3.14 source behavior
The [Glass Dev TUI guide](development-tui.md) covers recovery-sheet
interaction and the [Development Runtime guide](../development-runtime.md)
covers resident lifecycle and shutdown; this document defines connection
ownership and freshness boundaries.

## Ownership and lifecycle

There is no separate Chrome-owning controller in the source. The
`BrowserWorkspaceController` owns the UI state projection; the execution adapter
owns the session:

| Product path | Execution owner | Process ownership |
|---|---|---|
| Standalone `glass-browser` TUI | `BrowserTui.session: Option<BrowserSession>` | Owned launch owns Chrome; attach never owns the existing browser. |
| Glass Dev embedded App | `BrowserService` → one `BrowserWorker` → optional `BrowserSession` | Same owned/attached distinction; the resident worker serializes commands. |

Project files, editor buffers, PTYs, tasks, Pi conversations, and development
revisions outlive a browser session. A browser session owns target/frame route,
CDP state, observation caches, policy, browser revision, and (for an owned
launch) the Chrome child and profile lifecycle.

```text
DevelopmentWorkspace (survives browser loss)
├─ ProjectWorkspace / agents / processes / tasks
└─ BrowserService (embedded) or BrowserTui session (standalone)
   └─ BrowserSession generation
      ├─ owned Chrome + profile, or attached existing Chrome
      ├─ active target/frame route + browser revision
      └─ optional embedded-only RemoteView
```

The session starts its page revision at 1. Browser events and guarded actions
advance/invalidate it. A new target, navigation, reload, or page mutation makes
prior semantic references unsafe until a fresh observation is published.

## Connection states

`BrowserWorkspaceController` exposes the following bounded phases. The browser
session itself returns typed errors rather than silently changing phase.

| Phase | Authoritative data | UI and recovery behavior |
|---|---|---|
| `Detached` | No current session or revision | Browser controls explain how to start/attach; project remains usable. |
| `Starting` | Adapter has queued startup | Input remains responsive; startup errors become a visible status/recovery reason. |
| `Connected` | Session generation, endpoint summary, ownership, target route, current revision | Observe before acting; semantic and guarded mutation controls become available. |
| `Recovering` | Last bounded reason; semantic freshness invalidated | Embedded App offers recovery choices; standalone keeps its TUI open and reports retry commands. |
| `Failed` | Non-recoverable reason, no authoritative page revision | Continue project work; a new explicit start/attach is required. |

`connected` increments the controller generation and clears recovery. `disconnected`
clears semantic entities and selection and sets `Recovering` or `Failed`; it does
not destroy the surrounding product workspace.

## Launch, attach, and endpoint evidence

Owned startup validates policy and options, obtains a per-port OS lock, checks
whether the requested loopback CDP port is occupied, launches Chrome, and waits
for a verified endpoint. Attach checks for a healthy endpoint and never claims
its process. `--attach` cannot be combined with incognito or a non-default
profile. An owned incognito launch uses a disposable user-data directory and
removes it during normal session cleanup.

The low-level `probe_local_endpoint` helper is a bounded, loopback-only discovery
operation: it uses a 750 ms timeout, limits each discovery response to 512 KiB,
accepts only a matching loopback browser WebSocket URL, and projects at most 64
page targets. It classifies an endpoint as `Free`, `CompatibleBrowser`,
`UnrelatedService`, or `Unknown`. A TCP listener alone is never attach authority.
`reserve_loopback_port` returns an OS-selected port and a listener; callers must
keep the reservation until launch to reduce, but not eliminate, a bind race.

The current TUI does not run a general automatic scan or bounded retry loop.
`launch auto` binds a local ephemeral port, releases it immediately, then starts
Chrome; a race or startup failure is surfaced for another recovery choice.

## Embedded recovery and target picker

Glass Dev converts browser tool connection/launch errors into an
`BrowserRecoveryOffer` and switches to App. If the error text indicates a
compatible DevTools endpoint, the offer provides:

1. attach after the human checks the endpoint,
2. launch an isolated browser on a free local port, or
3. retry the preferred port.

For other errors it offers isolated automatic-port launch or retry. Dismissal
leaves the project and agent running. This offer is a bounded TUI error-derived
recovery mechanism; it is not proof equivalent to a fresh endpoint probe.

The embedded target picker requests `glass.browser.targets` asynchronously from
the resident worker, stores at most 64 targets, filters by ID/title/URL, and
shows at most 16 matches. Displayed URLs pass the embedded `safe_browser_url`
projection, which removes query and fragment and limits the visible string to
2,048 characters. Selecting a target is a governed, confirmed operation; the
worker calls `select_target` and then performs a fresh observation. A target
that disappears closes neither the App nor the surrounding workspace.

Standalone uses the session directly. `targets` lists targets and `select ID`
changes the active route followed by `observe`; its current target listing
renders target URLs directly, so query/fragment secrecy is not guaranteed in
that terminal projection. Neither picker persists target metadata.

## Revision and authority contract

Every semantic entity is stamped with its browser observation revision. Browser
navigation, history, reload, stop-loading, click, type, scroll, coordinate click,
and semantic intent execution can require the expected revision. A mismatch
fails closed; the caller must observe or reconcile rather than replay a stale
reference. Geometry revision is a separate presentation token for visual input.

App mutations go through governed `glass.browser.*` tools and require mutation
authority plus confirmation. Agent browser ownership is explicit. Human takeover
pauses agent browser mutation until a checkpoint is reconciled and control is
returned. Standalone keyboard actions use the same session revision checks but
have no Glass Dev project/agent authority layer.

## Remote View (embedded Glass Dev only)

Remote View is an explicitly opened, same-session capability. It does not launch,
attach to, or navigate another browser and does not own a `BrowserSession`.
`glass-browser` standalone marks Remote View operations unavailable.

```text
BrowserWorker / BrowserSession generation N
             ├─ App/terminal semantic + visual projections
             └─ RemoteView (only after remote-open)
                127.0.0.1:ephemeral/{random-token}/
                ├─ HTML viewer (GET token path)
                ├─ latest PNG watch value
                └─ bounded WebSocket input queue
```

The service binds IPv4 loopback on an OS-selected ephemeral port, uses 32 random
bytes encoded without padding as its in-memory token, accepts at most four
clients, and times out the initial handshake after three seconds. It serves
`Cache-Control: no-store`, a restrictive content-security policy, and
`X-Frame-Options: DENY`. Non-loopback peers, wrong token paths, oversized or
invalid messages, and full input queues are rejected. The SSH hint is an
explicit local port forward; Remote View itself remains loopback-only.

Frames are PNG only and capped at 8 MiB of base64 data. A watch channel replaces
the previous frame, so clients receive latest state rather than an unbounded
history. The worker publishes an initial screenshot on open and republishes
after it drains remote inputs at the beginning of a subsequent browser command.
Remote input is capped to 16 KiB per message and has normalized click coordinates
in `[0,1]`, scroll deltas bounded to ±10,000, keys of 1–128 bytes, text of at
most 8 KiB, and a browser revision on every message. Click coordinates are
multiplied by the current CSS viewport; all remote browser calls enforce that
revision before CDP mutation. Remote input currently carries no independent
geometry or frame-generation revision.

Stop, worker shutdown, and explicit `remote-revoke` await service termination.
The token is not persisted. Frame bytes are transient in the service's watch
value and response path; the current source does not promise automatic
redaction for incognito/private screenshots or terminal/browser scrollback.
Treat a Remote View URL and image as sensitive until revoked.

## Failure and recovery states

- **Starting/loading:** project and Agent surfaces remain usable while the
  browser operation runs in the worker.
- **Port occupied:** owned launch fails; choose attach only after endpoint
  inspection, choose another port, or launch automatically.
- **Malformed/unknown endpoint:** fail closed; do not attach on a TCP success.
- **Target changed/disappeared:** invalidate selection and obtain fresh targets
  and observation.
- **Stale action/input:** reject with a revision error; observe again.
- **Browser loss:** clear semantic/visual freshness, retain project state, and
  use embedded recovery or standalone reconnect/start/attach.
- **Remote View failure/revoke:** stop the view without changing browser
  session ownership; reopen explicitly if the session is still connected.

## Contract tests

Source tests cover endpoint projection and bounded classifications, port
reservation, stale route/revision behavior, target selection, embedded recovery
choices, loopback/tokenized/revocable Remote View, latest-frame replacement,
input bounds, and same-session revision-bound input. They do not certify a
standalone Remote View, unrestricted remote binding, or secret-free terminal
scrollback.
