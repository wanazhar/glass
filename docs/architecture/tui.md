# Glass terminal UI

Status: Accepted

## Design principles

- Preserve the existing two-panel interface and command-first workflow.
- Never block input or rendering on browser I/O.
- Retain only bounded, compact page state in the UI process.
- Make action state auditable without showing hidden model reasoning.

## Overall structure

```text
┌──────────────────────────── Header: title / URL ───────────────────────────┐
├────────────── Activity ──────────────┬────────── Structured observation ───┤
│ completed / active / failed actions  │ compact page state; scrollable      │
├──────────────────────────────────────┴─────────────────────────────────────┤
│ Command input                                                               │
├──────────────────────────── Status / keybindings ──────────────────────────┤
```

The visual layout remains unchanged. "Agent Thoughts" is an activity stream: user commands, lifecycle events, action start/end, errors, and page updates.

## Ownership and flow

```text
crossterm input task ──> UI loop ── BrowserCommand ──> BrowserWorker
                             ^                             │
                             └──── BrowserEvent ────────────┘
```

The UI loop owns terminal state and `App`. `BrowserWorker` owns the `BrowserSession`; it accepts one operation at a time and sends lifecycle/results events through bounded Tokio channels. Neither task owns the other's state.

## Interactions

| Input | Scope | Behavior |
|---|---|---|
| Enter | command input | parse and enqueue one browser command |
| Esc | busy state | request cancellation; otherwise close error/quit |
| q / Ctrl-C | app | request worker shutdown and exit cleanly |
| PgUp/PgDn | observation | scroll bounded page state |

## State variants

- Loading: show connection status and activity entry while session starts.
- Busy: retain previous page state, animate a small status indicator, accept cancellation.
- Error: show the current error overlay and retain prior page state.
- Empty: show no-page-loaded text.
- Constrained: preserve command and status bars; panels may stack or truncate at small terminal dimensions.

## Runtime rules

- Render only when state changes, plus a bounded busy animation tick.
- Input polling runs outside the async render loop using existing crossterm/Tokio facilities.
- A terminal guard restores raw mode, alternate screen, and cursor on every error path.
- Screenshots are written to files by the worker; base64 image data is not held in `App`.

## Tests

Unit tests cover reducer state transitions and command parsing. A worker integration test delays a browser command and proves the UI can still process key events, render busy state, and request cancellation.
