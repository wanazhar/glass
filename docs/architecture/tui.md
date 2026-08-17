# Glass terminal UI

Status: Current standalone Browser TUI reference (the separate `glass-dev`
Development TUI is documented in [Development TUI](development-tui.md))

## Design principles

- Preserve the command-first workflow and semantic page projection.
- Keep semantic selection movement local; document the async browser operations
  that can still occupy the standalone event loop.
- Retain only bounded page text, status, command, and visual state.
- Make action state auditable without exposing internal decision data.

## Overall structure

```text
┌────────────────────────────── Header (5 rows) ─────────────────────────────┐
│ connection · revision · presentation · owner · focus · status              │
├────────────────────────── Semantic page / help ────────────────────────────┤
│ bounded observation, target selection, workflow state, or help             │
├────────────────────────────── Command (3 rows) ────────────────────────────┤
│ visual path · status · > command                                           │
└─────────────────────────────────────────────────────────────────────────────┘
```

The semantic page and header are the primary audit surface. They show
connection state, revision, selected entity, bounded observation, workflow
state, and the latest operation status without exposing raw implementation
payloads.

## Ownership and flow

```text
crossterm input ──> BrowserTui reducer ──> BrowserSession / workspace
                         │                         │
                         └──── Ratatui render <────┘
```

The standalone Browser TUI owns its `BrowserSession`, command buffer,
`BrowserWorkspaceController`, semantic page projection, and visual state in
one async application object. Browser controls and command submissions are
async methods on that object and are awaited by the event loop. This TUI does
not expose a generic cancellation-token worker boundary for every browser
command.

Semantic selection movement is local: arrow keys, `j`/`k`, mouse wheel, and
semantic clicks update the rendered selection without issuing a CDP highlight
request for every movement. `Enter` performs the selected revision-guarded
action. The `glass-dev` product uses a separate `SnapshotWorker` for governed
background jobs; do not conflate the two implementations.

## Interactions

| Input | Scope | Behavior |
|---|---|---|
| Enter | command input | parse and asynchronously execute one browser command |
| Esc | command/overlay | clear the command buffer and close the active workspace overlay |
| q / Ctrl-C | app | close the owned session and restore the terminal |
| arrows / `j`/`k` | semantic page | move the local selected entity when the command is empty |

`observe`, `dom`, and `snapshot` in the TUI all refresh compact `PageContext`.
The TUI is an operational dashboard, so full DOM inspection remains an explicit
CLI/MCP capability rather than an unbounded right-panel payload.

## Command inventory

The TUI exposes the common interactive and platform-console operations without
requiring raw JSON:

| Area | Commands |
|---|---|
| Navigation and observation | `navigate`, `observe`, `semantic`, `text`, `dom`, `screenshot` |
| Guarded interactions | `click`, `double click`, `hover`, `type`, `clear`, `check`, `uncheck`, `select`, `scroll`, `press`, `shortcut` |
| Browser handling | `accept-dialog`, `dismiss-dialog`, `dismiss-consent`, `evaluate` |
| Workflows and intent | `workflow`, `resolve-intent`, `intent execute` |
| Local platform state | `profiles`, `knowledge`, `daemon status`, `daemon doctor`, `daemon logs`, `daemon recovery` |

Target discovery, frame selection, storage, downloads, uploads, diagnostics,
policy configuration, certification, and extension administration remain
explicit CLI/MCP/library operations. Keeping those bounded interfaces out of
the command prompt avoids inventing a second syntax for their structured
inputs; their supported fields and semantics are documented in the respective
interface guides.

## State variants

- Loading: show connection status and activity entry while session starts.
- Busy: retain previous page state, animate a small status indicator, accept cancellation.
- Error: show the current error overlay and retain prior page state.
- Empty: show no-page-loaded text.
- Constrained: preserve command and status bars; panel content truncates to the available terminal viewport.

## Runtime rules

- The event loop renders before polling input and redraws on its bounded polling
  cadence.
- Browser controls and command submissions remain async, but slow browser I/O
  can occupy the event loop while that operation is awaited.
- Semantic selection movement is local-only and therefore remains immediate.
- ANSI live capture samples a bounded PNG into an `AnsiPane`; Herdr frames use
  a latest-frame queue. Live capture is explicit and disabled by default.
- The semantic text, status, command input, and visual pane are bounded before
  rendering.
- A terminal guard restores raw mode, alternate screen, cursor, and graphics
  state on normal exit, Ctrl-C, quit, and close.

## Tests

Unit tests cover reducer state transitions, command parsing, responsive
classes, semantic selection, live-path choice, and visual quality bounds. The
TUI also provides read-only local daemon inspection commands: `daemon status`,
`daemon doctor`, `daemon logs`, and `daemon recovery`. These commands
render bounded JSON in the inspector pane without starting a browser operation.

## Remote and phone presentation

The standalone Browser TUI has three width classes: phone at 72 columns or
fewer, compact through 109 columns, and desktop above that. Its layout contains
a bordered five-row header, a bounded semantic/content pane, and a three-row
command footer. The header shows connection, browser revision, presentation,
input owner, focus, and the current status. The footer shows the visual path,
status, and command buffer.

The standalone Browser TUI has no development-surface navigation. Its command
buffer supports browser navigation, observation, semantic inspection, target
selection, workflows, and live presentation. `j`/`k`, arrows, mouse wheel, and
semantic clicks move local selection; `Enter` activates the selected target.
`Alt-Left`/`Alt-Right` perform guarded history navigation and `Ctrl-R` reloads.
`Esc` clears the command buffer and closes the current workspace overlay.

Live pixels are explicit and bounded. The visual path is selected from the CLI
policy: Herdr when available, otherwise ANSI when allowed. ANSI decodes a
bounded screenshot into an `AnsiPane`; Herdr receives latest-frame-only PNG
messages. Live capture is disabled by default, and an unavailable capture
reports a visible failure rather than silently claiming that live view is
active. The development product's phone layout and `SnapshotWorker` are
separate; see [Development TUI](development-tui.md).

The browser connection controller keeps semantic state and revision failures
visible after a disconnect. Launch, attach, reconnect, stop, targets, and
Remote View remain explicit commands. See [Browser connection controller and
Remote View](connection-presentation.md) and [Mobile and Remote Development](../mobile-remote.md).
