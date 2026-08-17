# Mobile and Remote Development

Glass has a geometry-responsive terminal workspace for SSH, Mosh, and narrow
local terminals. “Phone” means a single-pane terminal layout. It does not add a
second mobile authority path and it does not require touch events.

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

| Key | View | Purpose |
|---|---|---|
| `1` | Agent | readiness, conversation, and agent controls |
| `2` | Code | bounded files, editor buffers, and diagnostics |
| `3` | App | semantic browser state, workflow, and live view |
| `4` | Tasks | task state, verification, and task actions |
| `5` | More | workspace status, kernels, experiments, and operations |

`Tab` and `Shift-Tab` cycle the same phone destinations. The status footer
shows the current operation and the navigation hints. Status and error text is
not hidden behind the navigation row.

## Input contract

The phone layout uses the same keyboard and authority rules as desktop:

| Input | Behavior |
|---|---|
| `a` | open the current surface's action menu |
| `:` | open the governed command palette |
| `?` | open scrollable keyboard help in navigation mode |
| `j`/`k`, arrows, mouse wheel | scroll content; App `j`/`k` moves semantic selection |
| `i` on Agent | open the agent composer |
| `Enter` on Code/App | open a file or activate the selected browser entity |
| `d` on Git | queue and display the inline diff |
| `v` on App | toggle the bounded ANSI live view |
| `H` / `G` on App | take human control / reconcile Glass control |
| `Esc` | close the active menu, composer, palette, editor, diff, recovery sheet, or confirmation |
| `Ctrl-C` | restore the terminal and quit immediately |

The composer, palette, and editor keep printable characters such as `?` as text.
Action-menu commands remove documentation placeholders such as `NAME` and
`QUERY` before opening the editable palette input. If a background operation is
active, a new composer submission remains in the draft rather than being
silently discarded.

Mouse input is optional. The TUI can select navigation entries and semantic
browser entities when the terminal forwards mouse events. A mobile SSH client
that does not forward mouse events still has complete printable-key access.

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
opens a recovery sheet on the App surface. Choices are attach to a compatible
endpoint, launch on an automatic free port, try an explicit port, retry the
preferred port, or dismiss, depending on the detected failure. Browser launch
runs through the governed worker and the sheet never destroys files, tasks,
agent state, or Git state.

## Live browser view

Semantic browser state is the default. Embedded App live view is toggled with
`v` and uses the bounded ANSI half-block path in the current `glass-dev` TUI.
The screenshot worker keeps only the latest frame. If capture fails, Glass
clears the live-view toggle and shows the failure in the status footer instead
of leaving a stale “starting” state.

The standalone Browser TUI also supports explicit live presentation:

```console
glass-browser --tui-live on
glass-browser --tui-live-backend ansi --tui-live-quality data
```

Herdr is used when available and ANSI is the portable fallback. Standalone
semantic selection is local-only; moving through entities does not send a CDP
highlight request for every arrow key. `Enter` performs the selected action
through the normal revision guard.

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
Glass does not duplicate the multiplexer or silently manage its server.

For a full-fidelity iPhone browser, use the stable `safari` workflow or the
scoped `browser remote-view open` command. Both are loopback/SSH-forward
workflows. Keep the Chrome CDP port and application server private.

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

## Open the full application in Safari

Use `safari` for the stable application-server iPhone workflow or
`browser remote-view open` for the scoped current BrowserSession view. Both
work through a private SSH local port forward; neither exposes Chrome CDP.

## Further reading

- [Development Runtime](development-runtime.md)
- [Development TUI architecture](architecture/development-tui.md)
- [Harness architecture](harness-architecture.md)
- [Browser connection and Remote View](architecture/browser-connection.md)
- [CLI reference](cli.md)
- [Workspace trust](workspace-trust.md)
