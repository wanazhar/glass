# Mobile and Remote Development

Glass has a geometry-responsive terminal workspace for SSH, Mosh, and narrow
local terminals. “Phone” means a single-pane terminal layout. It does not add a
second mobile authority path and it does not require touch events.

**Status: Current 0.3.14 source behavior.** This describes the implemented
geometry-responsive TUI and development-TUI Remote View in this checkout.
Earlier card-based mobile designs are historical and immutable.


## Start on an iPhone

Automatic layout uses phone below 72 columns or 22 rows, compact below 118
columns or 32 rows, and desktop otherwise. Force a layout when the terminal
reports misleading geometry:

```console
glass --tui-layout mobile
glass --tui-layout compact
glass --tui-layout desktop
```

Phone mode keeps the same five primary destinations and adds a direct command
menu for terminals that do not send Alt/Ctrl-Tab reliably:

```text
[header]
[Agent | Code | Browser | Terminal | Git]  ← / in Agent or Ctrl-Tab
[status / command palette / composer]
```

| Key | View | Purpose |
|---|---|---|
| `Alt-1` | Agent | readiness, conversation, and agent controls |
| `Alt-2` | Code | bounded files, editor buffers, and diagnostics |
| `Alt-3` | Browser | semantic browser state, workflow, and selected live view |
| `Alt-4` | Terminal | managed project processes, logs, and exit states |
| `Alt-5` | Git | change list, selected diff, staging, and commit |


## Input contract

The phone layout uses the same keyboard and authority rules as desktop:

| Input | Behavior |
|---|---|
| `a` | open the current surface's action menu |
| `/` on Agent | open GC and Glass actions; filter by owner and press Enter to dispatch |
| `Ctrl-K` | open the Glass action palette on workspace surfaces |
| `?` | open scrollable keyboard help in navigation mode |
| `j`/`k`, arrows, mouse wheel | scroll content; Browser `j`/`k` moves semantic selection |
| printable text on Agent | open the composer and insert the text when the resident engine is ready |
| `Enter` on Agent | submit the focused composer; empty input stays in the conversation |
| `Enter` on Code/Browser | open a file or activate the selected browser entity |
| `d` on Git | queue and display the inline diff |
| `Alt-1`…`Alt-5` / `Ctrl-Tab` | switch primary workspace surfaces; `/` offers an accessible menu |
| `Ctrl-K` on Browser | open Start, Navigate, Observe, Targets, or View actions |
| `H` / `G` on Browser | take human control / reconcile Glass control |
| `Esc` | close the active menu, composer, palette, editor, diff, recovery sheet, or confirmation |
| `Ctrl-L` | open the shared composer dock |
| `Ctrl-C` | open Glass quit confirmation, including from the editor |
| `/` → Glass → Review workspace changes | open the evidence-aware review |
| `/` → Glass → Harness catalog | inspect installed external harnesses (informational only) |

In the full-screen Code editor, `Alt-W` toggles soft wrapping (off by default);
on, lines wrap at whitespace where possible with continuation gutters and
synchronized cursor/selection/highlighting; off horizontally scrolls source
columns. `Ctrl-S` saves, `Ctrl-Z`/`Ctrl-Y` undo/redo, and `Alt-A` sends the
focused editor context to the resident agent with a do-not-edit prompt. The editor starts in
INSERT. `Esc` returns to NORMAL. `Esc` from NORMAL on a clean buffer leaves the
editor. Unsaved buffers offer `S` save, `D` discard, `Q` discard-and-quit, or
`Esc`/`N` stay. `Ctrl-C` opens Glass quit confirmation from editor input; an
already-open unsaved-exit prompt keeps its save/discard/stay choices.
The composer, palette, and editor keep printable characters such as `?` as text.
Action-menu commands remove documentation placeholders such as `NAME` and
`QUERY` before opening the editable palette input. If a background operation is
active, a new composer submission remains in the draft rather than being
silently discarded.

Mouse input is optional. The TUI can select navigation entries and semantic
browser entities when the terminal forwards mouse events. A mobile SSH client
that does not forward mouse events still has complete printable-key access.
On first run Glass opens directly into the workspace. On Agent, type or press
`Enter`; when the resident engine is not ready, Glass reports the exact repair
step for Glass Core. Provider setup stays in Core, and readiness checks do not
print credentials.
Composer `Enter` sends and stays open; `Esc` closes. A local or pre-dispatch
failure keeps the draft for retry, and background work does not silently
discard typed text. If Core returns an error after accepting a prompt, Glass
marks that attempt failed, returns the agent to idle, and lists it in Attention
Inbox. Inspect the conversation before choosing Retry failed or uncertain
message from `/`; a confirmation is
required, and the prompt is never replayed automatically.


## Responsive execution

The first cockpit frame is rendered before the initial full workspace
projection. `SnapshotWorker` then hydrates files, conversation, tasks, Git,
processes, browser state, and other resident surfaces. Refresh requests and
conversation requests are coalesced, and the latest versioned snapshot wins.

Git diffs, browser recovery, agent composer operations,
embedded screenshots, and governed tool calls do not run on the terminal input
loop. TUI workspace callbacks use non-blocking lock attempts. When the actor is
busy, Glass reports a wait state rather than freezing input. Active bounded
jobs do not delay terminal restoration on quit.

The terminal guard owns raw mode, alternate screen, mouse capture, focus
reporting, and bracketed paste. It restores all of them on normal exit, error,
Ctrl-C, and quit. It does not persist composer drafts, command input, pixels,
cookies, temporary ports, or browser target IDs.

## Recover the browser without leaving the TUI

Glass opens directly into the project workspace. Glass Core owns resident
coding tools and their permission decisions; Glass retains its workspace and
browser integration controls and their normal confirmations.

A browser startup collision or disconnect keeps the project workspace alive and
opens a recovery sheet on the Browser surface. With a compatible endpoint, choices
are attach after checking the running browser, launch an isolated browser on an
automatic free port, retry the preferred port, or dismiss. Without one, choices
are launch on an automatic free port, retry the preferred port, or dismiss.
From the Browser action palette, `Start browser` starts the browser;
automatic port selection is a recovery-sheet choice, not an inline
automatic-port launch command. Browser startup runs through the governed worker
and the sheet never destroys files, tasks, agent state, or Git state.

## Live browser view

Semantic browser state is the default. In the development TUI, open Browser
actions with `Ctrl-K` and choose `Live browser view` to toggle the selected
backend. `--tui-live off` is default;
`auto` enables continuous frames only when the selected native path is
available, while `on` permits bounded ANSI fallback. `--tui-live-backend auto`
prefers Herdr, then Kitty, then ANSI; explicit Kitty emits terminal graphics,
explicit ANSI renders true-color half-blocks, and unavailable Herdr remains
semantic-only. Quality `data`, `balanced`, and `smooth` targets approximately
3/6/12 FPS. The worker keeps only the latest frame. Capture failure clears the
toggle and reports the failure rather than leaving stale “starting” state.

The standalone Browser TUI supports explicit live presentation:

```console
glass-browser --tui-live on --tui-live-backend kitty tui
glass-browser --tui-live on --tui-live-backend ansi --tui-live-quality data tui
```

Use `live on`/`live off` in that standalone TUI. Herdr and Kitty are native
image paths; ANSI is the portable fallback. Standalone semantic selection is
local-only; `Enter` performs the selected action through the normal revision
guard. Standalone `glass-browser` does not provide Remote View; use the
development TUI routes below.

Continuous pixels remain off by default. Treat screenshots, DOM, profiles,
cookies, storage, evaluated values, and diagnostic logs as sensitive. Do not
expose Chrome CDP or Remote View publicly.

## Preserve work with Herdr

[Herdr](https://herdr.dev/docs/how-to-work/) can own the persistent PTY on the
machine where the project and credentials live:

```console
ssh you@workstation
cd /path/to/project
herdr
# In a Herdr pane:
glass --tui-layout mobile
```

Herdr is optional. tmux remains compatible. Mosh can carry the terminal session
on roaming links; use a separate SSH connection for any local port forward.
For a client-side browser view, open Browser actions with `Ctrl-K` and choose
`Open SSH browser view`, `SSH browser view status`, or `Close SSH browser view`.
The service binds to loopback; connect through an explicit SSH local-port
forward rather than exposing it publicly:

```text
ssh -L <local-port>:127.0.0.1:<remote-port> <user>@<host>
```

Glass prints a tokenized loopback URL and a hint equivalent to:

```console
ssh -N -L PORT:127.0.0.1:PORT USER@HOST
```

Run the forward from the iPhone-side network and open the printed local URL in
Safari. Remote View is loopback/SSH-forward only; keep Chrome CDP and the
application server private.


## Current limits

The current phone TUI is not the earlier card-based Overview design. It has no
six-view touch dock, `inbox`, `notify`, or touch-only action authority. Those
concepts remain historical design material in
[the mobile cockpit architecture note](architecture/mobile-cockpit.md); the
implemented surface contract is documented in
[Development TUI architecture](architecture/development-tui.md).

## Terminal compatibility

- Herdr, tmux, SSH, and Mosh remain transport/PTY concerns outside the TUI
  authority model.
- Mouse, focus, and bracketed paste are optional terminal capabilities.
- ANSI live presentation is the portable fallback when native graphics are not
  available.
- Use a UTF-8 locale because the TUI uses Unicode state markers but does not
  rely on color alone.

## Open the full application from a phone

For the stable application-server iPhone workflow, use a private SSH local
port forward. For the scoped current browser-session view, use the Browser
actions for Open/Status/Close SSH browser view. Remote View remains loopback-only and does not
expose Chrome CDP.

## Further reading

- [Development Runtime](development-runtime.md)
- [Development TUI architecture](architecture/development-tui.md)
- [Harness architecture](harness-architecture.md)
- [Browser connection and Remote View](architecture/browser-connection.md)
- [CLI reference](cli.md)
- [Customization governance](customization-governance.md)
