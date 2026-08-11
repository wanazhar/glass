# Development workspace TUI layout

Status: Implemented 0.3.5 contract

The Development mode is a native Ratatui view layered beside the existing
Browser Workspace. It does not replace browser authority or make screenshots
the default context.

The resident development TUI exposes first-class Trust, Agent, Tasks, Editor,
LSP, Processes, Browser, Workflow, Debugger, Git, Tests, Kernels, Experiments,
Graph, Replay, and Daemon/Workspace surfaces. The command palette sends
governed operations through the same `DevelopmentToolRouter` used by CLI, MCP,
Pi, kernels, and daemon clients; a TUI mutation does not gain separate
authority.

```text
┌─ Glass — Development ─ project / branch ───────────────────────────────┐
│ FILES / GIT │ NATIVE EDITOR                         │ RUNTIME / TESTS   │
│ ▾ src       │  1 │ fn main() {                     │ ● dev healthy     │
│  M main.rs  │  2 │   checkout();                   │ ✓ unit 81/81      │
│  ! lib.rs 2 │                                      │ ACTORS             │
│              ├─ LIVE APP / SEMANTICS ───────────────┤ ◆ Human            │
│              │ action.checkout.submit · rev 582     │ ◆ Glass Agent      │
├──────────────┴───────────────────────────────────────┴───────────────────┤
│ AGENT / ATTRIBUTED TIMELINE                                             │
├──────────────────────────────────────────────────────────────────────────┤
│ project ... | browser revision | PTY health | Enter command              │
└──────────────────────────────────────────────────────────────────────────┘
```

## Interactions

| Input | Scope | Behavior |
|---|---|---|
| `F7` | desktop | enter Development mode |
| `F1`–`F6` | global | return to the existing browser surfaces |
| `1`–`6` | phone, empty command | switch Overview, Agent, Browser, Project, Diff, and Process |
| `Tab` / `Shift-Tab` | phone | cycle the single-pane phone views |
| `?` | phone | show or hide the phone control guide |
| `:` | global | open the filtered command palette |
| `j` / `k` | global, empty command | cycle every resident development surface |
| `view NAME` | command palette | drill into any named resident surface |
| `workspace` / `daemon` | command palette | inspect stable workspace identity, trust, generation, and resident recovery state |
| `safari` | command area | show private SSH port-forwarding instructions |
| `inbox` / `notify on` | command area | show attention groups or opt into a deduplicated terminal bell |
| `tap` / `tap N` | Browser | show and activate bounded revision-bound semantic actions |
| `verify card` | Diff | show compact code/runtime/semantic/visual evidence |
| `capsule save\|show\|clear` | command area | manage non-sensitive restart continuity |
| `project` | command area | show detected project configuration |
| `project open PATH` | command area | open a bounded native buffer |
| arrows/type/`Ctrl-S` | editor | navigate, edit, and atomically save |
| `Ctrl-Z` / `Ctrl-Y` | editor | bounded undo and redo |
| mouse click | editor/live app | place editor cursor or route browser input |
| `project search QUERY` | command area | fuzzy-search the development model |
| `project edit PATH CONTENT` | command area | save an actor-attributed buffer edit |
| `project run NAME COMMAND` | command area | start a managed PTY process |
| `project processes` | command area | inspect bounded process state |
| `project agent PROMPT` | command area | stream local harness events and tool results |
| `project pi ACTION` | command area | queue a real Pi RPC request without blocking input |
| `Y`/Enter or `N`/Esc | Pi approval sheet | approve the displayed exact mutation once, or deny it |
| `project diagnostics PATH` | command area | run LSP work off the input/render loop |
| `Esc` | busy/error | cancel browser work or dismiss an error |

## State variants

- **Loading**: project panes show `Project detection pending` while the
  browser worker connects independently.
- **Empty**: no open buffer or managed process is shown as an explicit empty
  state; the command hint remains visible.
- **Error**: bounded error text appears in the existing overlay and the event
  is retained in Activity.
- **Busy**: browser operations retain cancellation and mutation leases. LSP
  and Pi work run on bounded worker channels, so editor, render, and browser
  input continue to be polled.
- **Approval**: a Pi mutation blocks only its agent tool execution while the
  rest of the cockpit keeps rendering and accepting input. The sheet shows
  bounded, redacted effect evidence and expires fail-closed after 120 seconds.
- **Phone**: at 72 columns or fewer, or by explicit user preference, one
  full-width view is visible. Browser context remains semantic and
  continuous visual streaming is disabled by default. `--tui-live on` adds an
  adaptive live Browser view without changing the single-pane navigation model.
  An explicit `--tui-layout mobile` override is available.
  Overview orders workspace status, task state, agent activity,
  semantic/browser state, process/test health, and recovery/trust decisions.
  Every full surface remains available by cycling or `view NAME`, rather than
  compressing desktop panels into the phone pane. Browser can layer a numbered semantic
  action overlay above semantics or live pixels; the target reference retains
  the observation revision and fails closed when stale.
- **Compact**: terminals from 73 through 109 columns use a condensed workspace.
  Navigation and the selected full surface remain visible; the desktop-only
  context column is omitted.
- **Wide**: terminals at 110 columns or more use the complete development
  composition below. `--tui-layout desktop` forces this presentation.

Transport and graphics policy never select the layout. A phone layout may use
local graphics; a wide SSH layout may use semantic-only browser presentation.
Browser recovery appears as a focused dialog on compact/wide layouts and as an
attention decision sheet on phone layouts. It never destroys project state.

The runtime panel distinguishes confirmed state (`◆`, `→`, `✓`) from inferred
source/runtime links. A link never becomes certain merely because it is shown
in the TUI.

The Development layout assigns 21% to the file/Git surface, 54% to the editor
and live app, and 25% to runtime/tests/actors. The lower 24% is the agent and
attributed timeline. On constrained terminals, content stays bounded and the
browser-only modes remain one key away. The browser graphical geometry is
recomputed against the live-app rectangle, never the editor rectangle.

Herdr, tmux, and Mosh are transport and PTY-lifecycle layers rather than Glass
browser authorities. Herdr is the recommended mobile development multiplexer
because it combines persistent panes, agent state, remote attach, a narrow
terminal switcher, and an owned experimental graphics layer. Glass may stream
ephemeral live frames to that layer, but does not invoke or control the Herdr
server. tmux and Mosh retain the ANSI/Ratatui path.

## Verification

Deterministic Ratatui buffer tests render every resident surface at desktop,
compact, and phone geometries. Separate phone coverage proves executable
project configuration opens on the trust decision before activation. These
tests validate navigation and decision availability; they do not claim that a
local TUI process is itself a daemon transport or that remote connectivity has
been exercised.
