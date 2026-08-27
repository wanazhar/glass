# Glass Product Workspace

Status: Current 0.3.13 source behavior

## Product boundary and authority

Glass Dev is the development product; `glass-browser` is an independently
installable browser product. Glass Dev's `DevelopmentWorkspace` is the sole
authority for project identity, trust, files, editor buffers, processes, Git,
LSP/DAP, tasks, event history, agent sessions, and the resident browser service.
`DevTuiState` caches projections for rendering but does not become a second
owner.

```text
DevelopmentWorkspace (resident authority)
├─ ProjectWorkspace
│  ├─ files, buffers, cursor/selection, undo/redo, diagnostics
│  ├─ editor comments, proposals, checkpoints
│  ├─ CollaborationBus (claims + bounded subscribers)
│  ├─ Git, source/runtime graph, timeline, LSP
│  └─ ProcessManager / PTYs
├─ agent workers and pinned Pi runtime
├─ BrowserService → one BrowserWorker → optional BrowserSession
├─ task/workflow, kernel, debugger, experiment, replay state
└─ trust, policy, customization, GitHub configuration
             │
             v
       DevTuiState / DisplaySnapshot
       Agent · Code · App · Terminal · Tasks · Git · Debug · More
```

Destination changes only projection and focus. In particular, App does not move
browser state into `ProjectWorkspace`, and Code does not create an editor that
is separate from the project's shared buffers. The [Glass Dev TUI guide](development-tui.md)
and [Development Runtime guide](../development-runtime.md) define user-facing
keys and runtime lifecycle; this document records ownership and cross-surface
contracts.

## Product destinations

Desktop exposes eight destinations; phone exposes five. Advanced runtime,
workspace, replay, kernel, experiment, customization, and trust details remain
in More or the contextual command palette.

```text
Desktop: Agent · Code · App · Terminal · Tasks · Git · Debug · More
Phone:   Agent · Code · App · Tasks · More

Desktop
┌ GLASS · project · branch · trust/authority ────────────────────────────────┐
│ FILES / TASKS       PRIMARY SURFACE                 CONTEXT / EVIDENCE      │
│ project tree        Agent / Code / App / Terminal   selected symbol, event, │
│ and changes        Tasks / Git / Debug / More       browser entity, action │
├────────────────────────────────────────────────────────────────────────────┤
│ Agent  Code  App  Terminal  Tasks  Git  Debug  More              : palette │
└────────────────────────────────────────────────────────────────────────────┘

Phone
┌ GLASS · project · branch ─────────────┐
│ focused destination                   │
│ one scrollable primary pane           │
│ composer/status when active            │
├───────────────────────────────────────┤
│ Agent · Code · App · Tasks · More      │
└───────────────────────────────────────┘
```

The phone mode is a geometry-responsive single-pane TUI, not a separate touch
authority model. Every destination retains independent scroll state. Trust and
confirmation/recovery overlays own focus until accepted, denied, or dismissed.

## Startup and resident lifecycle

Opening a project detects project metadata without executing a detected command.
The workspace starts untrusted unless its filesystem identity matches the local
trust store. Static inspection, Git metadata, configuration review, and manual
browser use remain possible; project hooks, Pi, skills, tools, tests, LSP/DAP
overrides, kernels, and experiments wait for a trust decision.

Glass renders an initial cockpit before the full refresh. `SnapshotWorker` then
refreshes resident projections, agent conversation, browser screenshots, and
governed tool jobs off the input loop. Browser, editor, process, and agent
failures are projected as bounded states; they do not erase unrelated workspace
state. Workspace shutdown restores terminal modes and closes or detaches
owned runtime resources without waiting indefinitely on active UI work.

## Browser App integration

App is the development projection of the canonical
[`BrowserWorkspace`](browser-workspace.md). `BrowserService` runs in a named
resident worker (`glass-browser-workspace`) and serializes browser commands. It
owns the session generation, browser revision, optional owned Chrome PID,
workflow state, last start configuration, and optional Remote View. The project
workspace owns none of these values.

```text
App navigation / Agent context
            │
            v
BrowserWorkspaceController (selection, focus, ownership, bounded projection)
            │ governed glass.browser.* tool calls
            v
BrowserService → BrowserWorker → BrowserSession
                              ├─ owned Chrome + disposable/persistent profile
                              └─ attached Chrome (process not owned by Glass)
```

`browser navigate URL` may start a detached development browser, then performs
an observation before issuing the revision-bound navigation. Other actions
require a current observation revision. App target selection refreshes the
selected route and semantic observation. A browser generation can be stopped,
reconnected, or lost while project files, PTYs, tasks, and the agent continue.
See [Browser connection controller](browser-connection.md) for endpoint and
Remote View details.

App context sent to the Agent is bounded and explicit: current connection,
selected target, title, redacted page URL/origin, browser revision, semantic
summary, selected entity, workflow state, input owner, and freshness. An
actionable selected entity transfers browser mutation ownership to the Agent;
human takeover pauses browser mutation until reconciliation. Browser history is
not silently treated as current-page evidence.

## Code and editor collaboration

`ProjectWorkspace` owns a shared `EditorBuffer` per open path. A buffer includes
content (including unsaved content), original file hash, dirty state,
one-based cursor, optional anchor/active selection, and actor. The Code surface
renders the same buffer that file tools and Agent context read.

```text
Code surface ─┐
Agent context ─┼─> ProjectWorkspace buffers / revision ─> disk + timeline
file tools  ───┘                 │
                                ├─ CollaborationBus claims/events
                                ├─ anchored comments: Open → Resolved
                                ├─ proposals: Pending → Accepted/Rejected/Stale
                                └─ checkpoints: named buffer snapshots
```

| Artifact | Owner and revision rule | Conflict/recovery behavior |
|---|---|---|
| Cursor/selection | Buffer; one-based line and character column, UTF-8 boundary checked | Invalid positions are rejected; selected text is read from the shared unsaved buffer. |
| Edit claim | `CollaborationBus`; actor/path/ordered line range | Overlapping write claims from different actors fail; read claims and bounded events remain available. |
| Comment | Project workspace and durable timeline | Anchored to path and line range; open comments can be resolved by ID. |
| Proposal | Project workspace and durable timeline | Stores original/proposed text and base hash/revision. Accepting after content drift marks it stale and does not apply it. |
| Checkpoint | Project workspace and `editor-checkpoints.json` | Captures open buffers; restore records an event and adds undo history for replaced buffers. |
| Save/direct edit | Project workspace | Atomic save and external-change checks are authoritative; mutation remains governed and actor-attributed. |

Editor comments, proposals, and checkpoint commands are surfaced in Code's
Review panel. Agent chat attaches a bounded snapshot of the focused unsaved
buffer (up to 64 KiB), cursor, selection, dirty/actor state, and relevant
comments/proposals; it does not substitute an on-disk read for the human's
buffer. This editor context and browser context can coexist in one prompt, but
their revisions and authorities are independent.

## Agent, trust, and mutation authority

The ordinary Agent composer addresses the current resident conversation without
requiring an agent ID. Pi readiness and authentication are explicit setup
states. A prompt is held when another background action is active, and transport
failure preserves the draft for edit-and-retry. Browser, editor, Git, process,
workflow, and agent mutations all pass the governed worker/authorization path;
confirmation is one-use for the relevant frozen call. Raw JSON is an explicit
Inspect/export mode, not the default product surface.

```text
untrusted / Pi not ready ──> inspect or remediate ──> trusted + ready
                                                       │
                         ┌─────────────────────────────┴───────────────────────┐
                         │ human Code edits       Agent proposes browser/editor │
                         │ actor + confirmation   actor + revision + lease      │
                         └─────────────────────────────┬───────────────────────┘
                                                       v
                                             accepted / rejected / stale
```

A browser selected entity may make the browser owner `Agent`; `H`/`browser
human` changes it to Human, and `G`/`browser reconcile` requires the checkpoint
reconciliation before Glass ownership returns. Editor actor attribution and
browser input ownership are separate controls: taking browser control does not
unlock or overwrite an editor write claim.

## Workflow and evidence relationship

Browser workflows execute through the resident `BrowserService`, return step
results and a final browser revision, and expose list/pause/resume/cancel/verify
projections in App. They do not become project task evidence automatically.
Tasks, Git, editor events, browser observations, and workflow results are linked
through the development timeline and explicit evidence fields. A workflow or
browser action that loses its revision remains failed/stale rather than silently
replaying against a new page.

## Tests and limits

Source tests exercise destination and responsive rendering, trust/onboarding,
Agent context, shared editor selection and collaboration, stale proposals,
checkpoint restore, App semantic selection, browser revision guards, recovery,
and bounded asynchronous refresh. Tests also cover PTY cleanup, focus/mouse/
paste/resize paths, and 48x18, 64x24, and 80x24 phone flows. These architecture
contracts do not claim support for standalone Remote View, unimplemented image
protocols, or touch-specific behavior.
