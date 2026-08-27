# Canonical Browser Workspace

Status: Current 0.3.13 source behavior

## Purpose and boundary

`BrowserWorkspaceController` is the shared, human-facing state projection for the
focused `glass-browser` TUI and Glass Dev's embedded `App` surface. It owns
bounded presentation state, selection, focus, browser input ownership, and
recovery hints. It does **not** own Chrome, CDP, project files, editor buffers,
PTYs, Pi sessions, or workflow execution.

The two products use different execution adapters:

```text
standalone glass-browser                         glass (Glass Dev)
┌──────────────────────────────┐                 ┌──────────────────────────┐
│ BrowserTui                    │                 │ DevTuiState              │
│  BrowserSession (owned here)  │                 │  DevelopmentWorkspace    │
│  BrowserWorkspaceController  │                 │   ├─ BrowserService      │
│  Browser-only pages           │                 │   │   └─ BrowserWorker    │
└──────────────┬───────────────┘                 │   ├─ ProjectWorkspace    │
               │                                  │   ├─ agents / Pi          │
               v                                  │   └─ processes, Git, LSP  │
       browser semantic + visual                  │  BrowserWorkspaceController│
                                                  └─────────────┬────────────┘
                                                                v
                                                        App projection
```

The embedded adapter is resident: `BrowserService` serializes commands on one
worker and keeps one optional `BrowserSession`. The standalone TUI keeps its
session directly. Neither path creates a second browser authority when the
surface changes. See [Glass Dev TUI](development-tui.md) for keyboard and
surface behavior and [Development Runtime](../development-runtime.md) for
resident workspace lifecycle.

## Controller state and ownership

| State | Owner | Meaning and invalidation |
|---|---|---|
| Connection phase, generation, endpoint, ownership, recovery reason | Controller, fed by the adapter | `Detached`, `Starting`, `Connected`, `Recovering`, or `Failed`; a successful `connected` transition increments generation. |
| Page title, bounded URL, loading, browser revision | Browser session/adapter; projected by controller | Browser revision is the freshness token for semantic and mutation actions. |
| Targets and selected target | Browser session/adapter; projected by controller | At most 64 controller targets. Selecting a target must be followed by a fresh observation. |
| Entities, selected entity, semantic scroll/invalidation | Controller | At most 512 entities. Entity references are stamped with the observation revision. |
| Presentation path, reason, frame revision | TUI adapter/controller | Semantic-only is valid and often the default; pixels never replace semantic authority. |
| Focus, palette/address overlays, transient errors | Controller | Errors are bounded to eight entries; overlays own focus while open. |
| Browser input owner and takeover reconciliation | Controller | `Glass`, `Human`, and `Agent` are mutually exclusive. Agent-to-human takeover requires reconciliation before returning control. |
| Browser process, CDP route, workflow execution and checkpoints | Session or BrowserService | These are execution state, not controller-owned presentation state. |

`replace_entities` preserves a selection by reference when possible and otherwise
selects the first entity. A changed observation revision invalidates prior
entities. A stale action clears semantic selection and asks the adapter to
observe again. Text fields are bounded; state is not an event or frame history.

## Surfaces and layouts

The standalone TUI has Browser, Semantic, and Help modes. Its visual pane is
optional; its command line and semantic list remain usable without pixels.
Glass Dev's `App` is one destination among Agent, Code, Terminal, Tasks, Git,
Debug, and More. It renders browser visual/semantic state and a workflow summary;
it is not the standalone browser product embedded wholesale.

```text
Glass Dev App (desktop)
┌ BROWSER · loading/ready · Connected · redacted page path ────────────────────┐
│ ┌ VISUAL PLANE · semantic/ANSI/native ┐  ┌ INSPECTOR ─────────────────────┐ │
│ │ newest bounded frame, or reason      │  │ title · path · revision         │ │
│ │                                      │  │ selected entity · focus        │ │
│ └──────────────────────────────────────┘  │ semantic entities               │ │
│                                           └──────────────────────────────────┘ │
├ WORKFLOW · idle/running/completed/paused/failed/cancelled ───────────────────┤
└──────────────────────────────────────────────────────────────────────────────┘

Glass Dev App (phone/compact)
┌ BROWSER · connection · page/revision ─────┐
│ visual or semantic browser pane            │
│ (semantic is the reliable default)         │
├ WORKFLOW / status                           ┤
└────────────────────────────────────────────┘
```

The standalone command area provides `launch auto`, `launch PORT`, `attach
PORT`, `reconnect`, `stop`, `targets`, `select ID`, `navigate URL`, `observe`,
`semantic`, `screenshot`, and `live on|off`. Its direct target listing currently
renders the session's target URL; the embedded projection strips query and
fragment before display. This difference is intentional documentation of the
current source, not a claim that the standalone output is a redaction boundary.

## Revision-bound interaction

An action generated from a visible entity carries that entity's displayed
browser revision. Navigation, history, reload, stop-loading, click, type, and
scroll are rejected by the browser session when the expected revision is no
longer current. The controller refuses to create most browser actions without a
revision or without a selected actionable entity.

```text
observe fresh ──> revision R + entities stamped R
       │
       ├─ select target ──> fresh observe ──> revision R'
       ├─ human/Glass/agent action(expected R)
       │       ├─ accepted ──> browser changes ──> revision R+1 ──> observe
       │       └─ stale ─────> invalidate entities ──> observe again
       └─ resize/visual geometry ──> visual geometry revision (separate token)
```

The App submits browser operations through governed `glass.browser.*` tools;
mutating operations require mutation authority and one-use confirmation. The
standalone TUI executes its session directly and has no development workspace
or Remote View. Human takeover pauses agent browser mutation; `reconcile_takeover`
returns ownership to Glass only after the checkpoint is reconciled.

## Target selection and recovery

The embedded target picker loads targets asynchronously, filters by target ID,
title, or URL, shows at most 16 matches, and queues selection through the
confirmation path. Closing or a disappeared target leaves the App open. The
controller itself caps target storage at 64. The session's selected target and
frame route are the execution authority; the UI's `selected` marker is only a
projection.

Connection failures do not destroy project, editor, process, task, or agent
state. Glass Dev opens an App recovery offer with attach (when the error looks
like a compatible DevTools endpoint), an isolated automatic-port launch, retry,
or dismissal. Standalone reports the failure in its status line and supports
`reconnect`, `launch auto`, `launch PORT`, and `attach PORT`; it has no embedded
recovery sheet. Continue with semantic-only state when the visual backend fails;
semantic invalidation still requires a fresh observation after browser loss.

## Visual state

`BrowserPresentationPath` can describe Herdr, Kitty, Sixel, ANSI, or
SemanticOnly, but current TUI execution implements Herdr, explicit Kitty, and
bounded ANSI only. Sixel and iTerm-style graphics are not implemented renderers
in this source. See [Connection-aware presentation](connection-presentation.md)
for the policy contract and backend boundaries.

Pixels are latest-state presentation, not action evidence. A frame carries
browser and geometry revisions; stale frames and input are rejected by the
presentation contract. The terminal visual plane may be absent, delayed, or
cleared while semantic and control state continues. Incognito/private frames
are transient runtime data when captured; Remote View and terminal renderers do
not persist them to disk, but the current source does not promise automatic
redaction from process memory or terminal scrollback.

## Contract tests

Source tests cover shared adapter revision selection, bounded entities/targets,
focus movement, takeover/reconciliation, unsupported capabilities, standalone
and embedded recovery transitions, target filtering/redaction in the embedded
projection, App rendering, and stale browser actions. The intended integration
matrix includes desktop, compact, phone, and 48x18/64x24/80x24 layouts, plus
PTY cleanup, resize, mouse, paste, and Ctrl-C where the host supports them.
These tests do not certify unsupported Sixel/iTerm renderers or a standalone
Remote View.
