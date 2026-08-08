# Mobile and Remote Development

Glass has a phone-oriented terminal workspace for SSH and Mosh sessions. It is
designed for steering an agent, reviewing changes, checking runtime state, and
verifying the application. It does not compress the desktop workspace into
several unreadable columns.

## Start on an iPhone

The automatic layout uses the phone workspace at 72 columns or fewer. In an
SSH or Mosh session it keeps that layout through 96 columns, which covers most
portrait and landscape phone terminals. Force either presentation when needed:

```console
glass --tui-layout mobile
glass --tui-layout desktop
```

The phone workspace starts in Development mode and provides five full-width
views:

| Key | View | Purpose |
|---|---|---|
| `1` | Home | Connection, browser, runtime, test, and actor status |
| `2` | Agent | Agent activity and attributed timeline |
| `3` | App | Structured browser semantics and Safari handoff |
| `4` | Diff | Current project and verification diff |
| `5` | More | Files, editor state, processes, diagnostics, and actors |

`Tab` and `Shift-Tab` move between views. `Esc` returns to Home before it
exits, and `?` shows the phone key guide. The number keys switch views only
while the command bar is empty, so commands and agent prompts can contain
numbers normally. Function keys, pointer input, and control-key chords are not
required for navigation.

The phone layout is semantic-first. It does not start the continuous PNG
screencast or allocate a terminal graphics pane. This saves Chrome encoding,
network bandwidth, and terminal redraw work. `screenshot PATH` remains an
explicit capture request.

## Preserve work with Herdr

[Herdr](https://herdr.dev/docs/how-to-work/) is the recommended multiplexer for
Glass development sessions. Like tmux it owns persistent PTYs and supports
detach/reattach, but it also exposes agent state and has its own responsive
phone switcher. Glass detects `HERDR_ENV=1` and shows Herdr as the active
transport context.

Install Herdr on the machine where the code and credentials live, then:

```console
ssh you@workstation
cd /path/to/project
herdr
# In a Herdr pane:
glass --tui-layout mobile
```

Detach Herdr with `Ctrl-B`, then `q`. SSH back and run `herdr` to reattach.
Herdr preserves the pane and the Glass process; Glass does not duplicate or
silently manage Herdr's server.

Herdr is a better default than tmux for this particular agent-oriented mobile
workflow, not a mandatory Glass dependency. tmux remains compatible. Mosh is a
useful outer transport on roaming or lossy networks, and Herdr remains the
persistent PTY owner inside it:

```console
mosh you@workstation
herdr
```

Mosh replaces the interactive SSH transport with a UDP session after login.
Use an SSH client or a separate SSH connection for the TCP port forward needed
by Safari.

## Open the live application in Safari

An iOS SSH terminal cannot provide a portable live pixel browser surface.
Terminal image protocols vary by client, and a remote shell cannot launch the
local iPhone Safari application. Use Glass semantics in the TUI and an SSH
local port forward for the real application.

Configure `browserUrl` in `glass.toml`, or navigate to the application, then
enter this in the Glass command bar:

```text
safari
```

Glass prints the exact local and remote ports. In the iPhone SSH client, add a
local forwarding rule like:

```text
local port:  3000
remote host: 127.0.0.1
remote port: 3000
```

Keep the tunnel connected and open `http://127.0.0.1:3000` in Safari. The
route and non-sensitive query values are preserved in the generated URL;
credentials and secret-like query values are removed or redacted. The local
port may be changed in the SSH client when it conflicts with another service;
use that chosen port in Safari.

Do not expose the Chrome CDP port or bind a development server publicly for
this workflow. Herdr owns terminal persistence, while the SSH client owns the
private Safari tunnel.

## Terminal compatibility

- iOS terminal Smart Keys can still send arrows, `Esc`, and control keys, but
  Glass phone navigation does not depend on their arrangement.
- Mouse or touch events are optional. The numbered navigation row accepts
  clicks when the client reports mouse events.
- Orientation changes are handled through terminal resize events. The layout
  switches between phone, compact, and wide presentations in automatic mode.
- Use a UTF-8 locale. Glass uses Unicode state markers but never relies on
  color alone for meaning.
