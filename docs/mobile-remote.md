# Mobile and Remote Development

Glass has a geometry-responsive terminal workspace for SSH, Mosh, and narrow
local terminals. “Phone” means a single-pane terminal layout. It does not add a
second mobile authority path and it does not require touch events.

**Status: Current 0.3.13 source behavior.** This describes the implemented
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

Phone mode exposes five direct destinations:

```text
[header]
[Agent | Code | App | Tasks | More]  ← 1..5 or Tab/Shift-Tab
[status / command palette / composer]
```

| Key | View | Purpose |
|---|---|---|
| `1` | Agent | readiness, conversation, and agent controls |
| `2` | Code | bounded files, editor buffers, and diagnostics |
| `3` | App | semantic browser state, workflow, and selected live view |
| `4` | Tasks | task state, verification, and task actions |
| `5` | More | workspace status, kernels, experiments, and operations |


## Input contract

The phone layout uses the same keyboard and authority rules as desktop:

| Input | Behavior |
|---|---|
| `a` | open the current surface's action menu |
| `:` | open the governed command palette |
| `?` | open scrollable keyboard help in navigation mode |
| `j`/`k`, arrows, mouse wheel | scroll content; App `j`/`k` moves semantic selection |
| printable text on Agent | open the composer and insert the text when Pi is ready |
| `Enter` on Agent | open the composer or queue Pi setup when unready |
| `Enter` on Code/App | open a file or activate the selected browser entity |
| `d` on Git | queue and display the inline diff |
| `:browser view` | toggle the selected Herdr, Kitty, or ANSI live backend |
| `H` / `G` on App | take human control / reconcile Glass control |
| `Esc` | close the active menu, composer, palette, editor, diff, recovery sheet, or confirmation |
| `Ctrl-L` | open the shared composer dock |
| `Ctrl-C` | open Glass quit confirmation, including from the editor |
| `:review` | prefill an evidence-aware review prompt |
| `:harness list` | show installed external coding harnesses |
| `:harness start NAME` | hand the terminal to one installed harness, then resume Glass |

In the full-screen Code editor, `Alt-W` toggles soft wrapping (off by default);
on, lines wrap at whitespace where possible with continuation gutters and
synchronized cursor/selection/highlighting; off horizontally scrolls source
columns. `Ctrl-S` saves, `Ctrl-Z`/`Ctrl-Y` undo/redo, and `Alt-A` sends the
focused editor context to Pi with a do-not-edit prompt. The editor starts in
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
On first run, if Trust is required, choose the footer action before repository
execution. On Agent, type or press `Enter`; `:agent setup` installs/repairs
the pinned Pi runtime, `:agent setup login` opens Pi `/login`, and `:agent
update` refreshes it. Setup mutations use the one-use confirmation card.
Composer `Enter` sends and stays open; `Esc` closes. A failed send keeps the
draft for retry, and background work does not silently discard typed text.


## Responsive execution

The first cockpit frame is rendered before the initial full workspace
projection. `SnapshotWorker` then hydrates files, conversation, tasks, Git,
processes, browser state, and other resident surfaces. Refresh requests and
conversation requests are coalesced, and the latest versioned snapshot wins.

Git diffs, Pi runtime setup, browser recovery, agent composer operations,
embedded screenshots, and governed tool calls do not run on the terminal input
loop. TUI workspace callbacks use non-blocking lock attempts. When the actor is
busy, Glass reports a wait state rather than freezing input. Active bounded
jobs do not delay terminal restoration on quit.

The terminal guard owns raw mode, alternate screen, mouse capture, focus
reporting, and bracketed paste. It restores all of them on normal exit, error,
Ctrl-C, and quit. It does not persist composer drafts, command input, pixels,
cookies, temporary ports, or browser target IDs.

## Recover the browser without leaving the TUI

An untrusted repository starts on the Trust surface. The phone footer shows the
actual trust actions:

```text
I inspect · O open untrusted · 1 trust once · T trust project
```

Trust decisions update the authority label immediately. Repository-controlled
execution remains blocked until the local user makes a decision.

A browser startup collision or disconnect keeps the project workspace alive and
opens a recovery sheet on the App surface. With a compatible endpoint, choices
are attach after checking the running browser, launch an isolated browser on an
automatic free port, retry the preferred port, or dismiss. Without one, choices
are launch on an automatic free port, retry the preferred port, or dismiss.
From the development TUI command palette, `:browser start` starts the browser;
automatic port selection is a recovery-sheet choice, not an inline
automatic-port launch command. Browser startup runs through the governed worker
and the sheet never destroys files, tasks, agent state, or Git state.

## Live browser view

Semantic browser state is the default. In the development TUI, use
`:browser view` to toggle the selected backend. `--tui-live off` is default;
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
For a full-fidelity iPhone browser, use Remote View from the development TUI:

```text
:browser remote-open
:browser remote-status
:browser remote-revoke
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
port forward. For the scoped current BrowserSession view, use the development
TUI `:browser remote-open`, `:browser remote-status`, and
`:browser remote-revoke` routes. Remote View remains loopback-only and does not
expose Chrome CDP.

## Further reading

- [Development Runtime](development-runtime.md)
- [Development TUI architecture](architecture/development-tui.md)
- [Harness architecture](harness-architecture.md)
- [Browser connection and Remote View](architecture/browser-connection.md)
- [CLI reference](cli.md)
- [Workspace trust](workspace-trust.md)
