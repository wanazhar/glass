# Glass terminal UI

Status: Accepted

## Design principles

- Preserve the existing two-panel interface and command-first workflow.
- Never block input or rendering on browser I/O.
- Retain only bounded, compact page state in the UI process.
- Make action state auditable without exposing internal decision data.

## Overall structure

```text
┌──────────────────────────── Header: title / URL ───────────────────────────┐
├────────────── Activity ──────────────┬────────── Structured observation ───┤
│ completed / active / failed actions  │ compact page state; scrollable      │
├──────────────────────────────────────┴─────────────────────────────────────┤
│ Command input                                                               │
├──────────────────────────── Status / keybindings ──────────────────────────┤
```

The visual layout remains unchanged. "Activity" is an auditable event stream:
user commands, lifecycle events, action start/end, errors, and page updates.

## Ownership and flow

```text
crossterm input task ──> UI loop ── BrowserCommand ──> BrowserWorker
                             ^                             │
                             └──── BrowserEvent ────────────┘
```

The UI loop owns terminal state and `App`. `BrowserWorker` owns the `BrowserSession`; it accepts one operation at a time and sends lifecycle/results events through bounded Tokio channels. Neither task owns the other's state.

The worker runs on the TUI's Tokio `LocalSet`: its browser I/O remains fully
asynchronous and yields to the UI loop, without forcing the existing browser
error boundary across worker threads.

A small dedicated Crossterm input worker forwards key events to the UI reducer.
The reducer may enqueue at most one active browser operation and never awaits a
browser future. `Esc` sends cancellation for that operation's ID. Cancellation
is best effort: it drops Glass's in-flight operation future, but cannot roll
back a CDP input event that Chrome already received.

## Interactions

| Input | Scope | Behavior |
|---|---|---|
| Enter | command input | parse and enqueue one browser command |
| Esc | busy state | request cancellation; otherwise close error/quit |
| q / Ctrl-C | app | request worker shutdown and exit cleanly |
| PgUp/PgDn | observation | scroll bounded page state |

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

- Render only when state changes, plus a bounded busy animation tick.
- Input polling runs outside the async render loop using existing crossterm/Tokio facilities.
- Screenshots are written to files by the worker; base64 image data is not held in `App`.
- The right-panel string, header values, activity history, and command input
  each have explicit bounds before they enter `App` state.
- A terminal guard restores raw mode, alternate screen, and cursor on every
  normal error/exit path before waiting for browser-worker shutdown.

## Tests

Unit tests cover reducer state transitions and command parsing. A worker integration test delays a browser command and proves the UI can still process key events, render busy state, and request cancellation.
The TUI also provides read-only local daemon inspection commands: `daemon status`,
`daemon doctor`, `daemon logs`, and `daemon recovery`. These commands
render bounded JSON in the inspector pane without starting a browser operation.

## Remote and phone presentation

Automatic presentation has three terminal-width classes: phone through 72
columns, compact through 109 columns, and wide above that. SSH and Mosh extend
the phone threshold through 96 columns because landscape mobile terminals are
often wider while retaining phone input constraints. `--tui-layout mobile`
and `--tui-layout desktop` provide deterministic overrides.

Phone mode is a single-pane stack with Home, Agent, App, Diff, and More views.
Its browser worker does not start the screencast or screenshot fallback by
default, so a semantic-only iOS terminal pays no continuous visual capture or
transfer cost. Explicit screenshots still write requested evidence. An
explicit `live auto|on` starts a bounded, pane-sized screencast with data,
balanced, and smooth throttles. Delivery is latest-frame-only: Herdr uses a
one-frame local-socket queue, Kitty uses the terminal graphics mailbox, and the
ANSI renderer stores only sampled Ratatui cells. Live PNGs are never persisted.

Backend negotiation is conservative: owned Herdr graphics, then a direct Kitty
environment match or bounded terminal query, then ANSI only when `on` permits
it. Mosh suppresses native image auto-detection because it synchronizes the
terminal grid; ANSI remains available. Backend errors degrade in that same
order and cannot terminate the browser worker. `live doctor` exposes the
transport signals, selected backend, throughput, FPS, and drops. The `safari`
command remains the stable full-fidelity SSH-forward path.

Glass reads Herdr's documented pane markers for context and, when live pixels
are explicitly enabled, can open Herdr's experimental owned pane-graphics
stream. Process persistence and detach/reattach remain owned by Herdr rather
than a duplicate multiplexer inside Glass.
