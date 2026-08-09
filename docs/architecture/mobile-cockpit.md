# Remote development cockpit

Status: Accepted 0.3.3 redesign

## Purpose and boundaries

The cockpit is an intentionally remote presentation of the same Glass
workspace. It preserves agent, semantic, project, test, process and command
state without depending on browser pixels. It does not turn MCP into an image
transport, expose a public relay, or replace Herdr/Mosh/SSH as transport owner.

## Runtime ownership

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
             └── BrowserConnectionController (replaceable subsystem)
```

The resident project session survives browser disconnect/recovery. Capsules do
not persist command input, prompts, pixels, cookies, temporary ports/tokens,
browser PIDs or stale target IDs.

## Overall phone layout

```text
┌─ glass / checkout ─ SSH · semantic ┐
│ agent RUNNING · app ATTACHED · r83 │  sticky workspace/connection header
├─ NEEDS YOU (1) ────────────────────┤
│ ! browser port conflict      Enter │  urgent cards precede telemetry
├─ AGENT ────────────────────────────┤
│ observe → patch → HMR → verify     │
│ editing src/cart.rs                │
├─ LIVE APP ─────────────────────────┤
│ /checkout · rev 83 · fresh         │
│ visual assist: snapshot       Open │
├─ UNDERSTANDING ────────────────────┤
│ checkout.form · submit enabled     │
├─ TESTS / PROCESS ──────────────────┤
│ ✓ unit 81/81 · ● dev :3000         │
├────────────────────────────────────┤
│ 1 Overview 2 Agent 3 Browser       │
│ 4 Diff 5 Project 6 Process   : cmd │
└────────────────────────────────────┘
```

The Overview is an adaptive card stack. At normal phone heights it renders
separate needs-attention, live-app, agent, understanding, and process cards.
At short terminal heights it collapses to a two- or three-card priority window;
`PageUp` and `PageDown` page through the remaining cards. The project/status
header, command composer, navigation, and help/status footer remain visible.
Browser pixels are a preview or deliberate burst and cannot displace command
input, agent state, or process health.

The terminal-native rendering approximates the issue-pinned iOS/Android visual
language with rounded Unicode card borders, dark panel surfaces, cyan browser
and semantic state, purple agent state, green runtime/verification state,
status chips, a preview inset, and a visible command cursor. It does not claim
pixel identity with native phone controls.

## Focused views

```text
Overview ─ summary cards and attention
Agent    ─ current step, tools, approvals and reconciliation
Browser  ─ semantic page, compact preview, recovery and Remote View
Diff     ─ changed files and verification evidence
Project  ─ files, editor buffer and diagnostics
Process  ─ PTYs, tests, logs and lifecycle controls
```

Each view has loading, empty, busy, error and constrained states. Browser also
has probing, recovery, target-picker, disconnected, semantic-only and Remote
View-active states.

## Interaction contract

| Input or command | Behavior |
|---|---|
| `1`–`6` | Select a focused view without function/control keys. |
| `Tab` / `Shift-Tab` | Cycle focused views. |
| `:` | Open a filtered command palette with browser lifecycle actions. |
| `?` | Toggle contextual shortcut/help content. |
| `inbox` | Open the bounded attention summary. |
| `notify on\|off\|status` | Control deduplicated terminal-bell attention. |
| `tap` / `tap N` | Show/activate bounded revision-bound semantic actions. |
| `verify card` | Show compact code/runtime/semantic/visual evidence. |
| `capsule save\|show\|clear` | Manage non-sensitive reconnect state. |
| `live quality auto` | Apply connection-aware scale/rate adaptation and show its reason. |
| `browser ...` | Probe, launch, attach, pick target, reconnect, disconnect or select semantic-only. |
| `browser remote-view open` | Create scoped loopback view and SSH-forward guidance. |
| `Esc` | Close the focused overlay/sheet before affecting background work. |

No essential action requires function keys, mouse reporting or a terminal image
protocol. Touch/mouse may activate visible tabs/cards when the terminal emits
events.

## Semantic tap overlay

```text
┌─ Semantic actions · revision 83 ───┐
│ [1] Open menu                      │
│ [2] Search                         │
│ [3] Add to cart                    │
│ [4] Continue checkout              │
└────────────────────────────────────┘
```

Selection is bound to browser and geometry revisions. Stale targets fail
closed and prompt a fresh observation.

## Browser recovery sheet

```text
┌─ BROWSER NEEDS ATTENTION ──────────┐
│ Port 9222 is busy                  │
│ unrelated listener detected       │
│ project / agent / processes alive │
│                                   │
│ > Launch on automatic port        │
│   Inspect / attach                │
│   Choose port                     │
│   Semantic only                   │
│                                   │
│ Enter choose · Esc later          │
└───────────────────────────────────┘
```

The sheet closes without quitting Glass. Target selection is a bounded
filterable list with privacy-aware URL projection. The same controller actions
exist in compact/wide overlays and the command palette.

## Attention, cards and privacy

Attention is ordered: blocking confirmation/failure, current agent action,
live-app health, semantic freshness, process/tests, diff, low-level telemetry.
Items are deduplicated and bounded. Notification bodies contain no prompt,
process output, source content, secret-bearing URL, frame or token.

Verification cards contain bounded outcomes, changed-file count, semantic
revision and explicit visual status. Visual status is `not-captured` until an
explicit screenshot/comparison supplies evidence.

## Adaptive live view

Auto mode begins with the profile selected from independent transport/graphics
evidence. Pressure reduces capture scale before frame rate. Local
balanced/smooth target 30/60 FPS; 3/6/12 FPS profiles are constrained remote
visual-assist modes only. Hidden Browser views suspend terminal capture. A
manual selection disables adaptation until auto is restored.

## Tests

- A populated 46x50 Ratatui buffer test verifies the reference hierarchy,
  rounded cards, preview inset, palette, composer and navigation; a 40x20 test
  verifies bounded Overview paging, and existing compact/wide tests guard the
  shared responsive reducer.
- Reducer tests cover every printable navigation route, overlay focus, help,
  command filtering and semantic stale refusal.
- Browser recovery tests cover compatible/unrelated/unknown listeners, target
  selection, semantic-only and reconnect without project identity loss.
- Design-asset validation verifies decodable images; the populated phone buffer
  is asserted against their information hierarchy and a real 40x20 PTY test
  covers executable rendering and terminal restoration.
- Resident/capsule/inbox/card tests retain their bounds and privacy contracts.
