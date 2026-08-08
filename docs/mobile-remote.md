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
| `3` | App | Structured semantics, opt-in terminal live view, and Safari handoff |
| `4` | Diff | Current project and verification diff |
| `5` | More | Files, editor state, processes, diagnostics, and actors |

`Tab` and `Shift-Tab` move between views. `Esc` returns to Home before it
exits, and `?` shows the phone key guide. The number keys switch views only
while the command bar is empty, so commands and agent prompts can contain
numbers normally. Function keys, pointer input, and control-key chords are not
required for navigation.

The phone layout remains semantic-first: continuous pixels are off by default,
which saves Chrome encoding, network bandwidth, and terminal redraw work.
`screenshot PATH` remains an explicit evidence capture. Enable an ephemeral
terminal-native view when visual feedback is useful:

```console
glass --tui-layout mobile --tui-live on --tui-live-quality data
```

Or enter `live on` in the command bar. Glass switches to App, starts a bounded
PNG screencast, retains at most the current/pending frame, and never persists
those live pixels. Useful commands are `live status`, `live doctor`, `live
off|auto|on`, `live backend auto|herdr|kitty|ansi`, `live quality
data|balanced|smooth`, and `live fit contain|cover|actual`.

Fit controls apply to ANSI sampling. Herdr and direct Kitty use aspect-safe
contain placement so their native overlays and browser pointer geometry remain
aligned.

`auto` only enables a native Herdr or Kitty renderer that Glass can detect.
`on` additionally permits the portable true-color ANSI half-block renderer.
The data, balanced, and smooth profiles target approximately 3, 6, and 12 FPS
and adapt the bounded capture dimensions to the current App pane. Use data on
a cellular link; balanced is the general SSH default; smooth is intended for a
fast LAN.

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

Herdr can also own Glass's live image layer. Enable Herdr's experimental Kitty
graphics support in its configuration, then start Glass with `--tui-live auto`
or `--tui-live-backend herdr`. Glass connects to the pane-local
`HERDR_SOCKET_PATH`, opens one owned `pane.graphics.stream`, and sends raw PNG
frames on a one-frame queue. Detaching does not write image escape sequences
into shell history, and closing Glass releases the Herdr-owned layer. If the
stream fails, Glass falls back to direct Kitty when detected, then ANSI for
`live on`, and finally the semantic view.

## Open the full application in Safari

The terminal-native view is designed for steering and verification, not as a
replacement for a touch browser: terminal graphics support varies, ANSI frames
trade fidelity for portability, and a remote shell cannot launch local iPhone
Safari. The stable full-fidelity path is still an SSH local port forward.

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

- Herdr is the preferred owned graphics path. Direct Kitty is selected after a
  bounded capability probe; forcing `--tui-live-backend kitty` is available
  when a compatible client does not identify itself.
- Mosh synchronizes terminal cells rather than arbitrary image protocol state.
  `live on` therefore selects ANSI automatically under Mosh; use Safari over a
  separate SSH tunnel for full fidelity.
- tmux and other multiplexers work with ANSI. Direct Kitty passthrough depends
  on their configuration, so `live doctor` reports what Glass actually chose.
- iOS terminal Smart Keys can still send arrows, `Esc`, and control keys, but
  Glass phone navigation does not depend on their arrangement.
- Mouse or touch events are optional. The numbered navigation row accepts
  clicks when the client reports mouse events.
- Orientation changes are handled through terminal resize events. The layout
  switches between phone, compact, and wide presentations in automatic mode.
- Use a UTF-8 locale. Glass uses Unicode state markers but never relies on
  color alone for meaning.

Run the browser-free renderer benchmark on the remote machine with:

```console
GLASS_LIVE_BENCH_ITERATIONS=100 \
  cargo run -p glass-browser --release --example terminal_live_benchmark
```
