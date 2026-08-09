# Mobile and Remote Development

Glass has a phone-oriented terminal workspace for SSH and Mosh sessions. It is
designed for steering an agent, reviewing changes, checking runtime state, and
verifying the application. It does not compress the desktop workspace into
several unreadable columns.

## Start on an iPhone

Automatic layout uses phone at 72 columns or fewer, compact at 73–109, and
wide at 110 or more. Width is geometry only: SSH/Mosh never changes those
breakpoints. Force a presentation when geometry is misleading:

```console
glass --tui-layout mobile
glass --tui-layout compact
glass --tui-layout desktop
```

The phone workspace starts in Development mode and provides six full-width
views:

| Key | View | Purpose |
|---|---|---|
| `1` | Overview | Connection chips plus Live App, Agent, Understanding, and Tests & Process cards |
| `2` | Agent | Agent activity and attributed timeline |
| `3` | Browser | Structured semantics, live view, recovery, and Safari handoff |
| `4` | Project | Files, editor state, diagnostics, and actors |
| `5` | Diff | Current project and verification diff |
| `6` | Process | PTY health and bounded output |

`Tab` and `Shift-Tab` move between views. `Esc` returns to Overview before it
exits, and `?` shows the phone key guide. The number keys switch views only
while the command bar is empty, so commands and agent prompts can contain
numbers normally. `:` or `/` opens the command-palette hint, and `Ctrl-L`
redraws. Function keys, pointer input, and control-key chords are not required.

## Native-feeling phone controls

Every phone view has a one-row action dock whose commands depend on the active
surface. Browser exposes Back, Tap, live toggle, and Remote View; Agent exposes
Back, Timeline, Abort, and Actions; Project, Diff, and Process expose their
bounded read/verification operations. The dock is clickable when the SSH client
forwards mouse events and remains reachable through the documented commands.

Press `:` or `/`, or tap `Actions`, to open the searchable action sheet. Type to
filter, use Up/Down and Enter, tap a result, or press Esc to close it. Agent abort
uses a confirmation sheet with touch and `Y`/`N` routes. The command composer has
a real Send target which becomes Cancel while a browser operation is active.
Up/Down recalls up to 32 commands for the current process. History and command
drafts are deliberately memory-only because commands can contain secrets.
Bracketed paste is enabled only while Glass owns the terminal and input remains
bounded by the same 4 KiB composer limit.

Glass requests mouse, focus, and bracketed-paste reporting on entry and restores
all three on exit. Losing terminal focus suspends live acquisition; returning
queues a fresh structured observation before treating evidence as current.
Completed and failed operations use short-lived, bounded toasts. These behaviors
degrade to printable keys when a mobile SSH client does not forward the optional
terminal protocols.

Overview mirrors a native mobile cockpit without inventing state: the header
shows project, revision, connection, agent, and browser status; rounded cards
project the latest browser, agent, semantic, and process evidence; the bordered
composer and six navigation pills remain reachable at the bottom. Needs-attention
state takes priority above those cards. On very short terminals, `PageUp` and
`PageDown` move through a bounded card window instead of crushing every card into
unreadable rows. With terminal mouse reporting, tapping the Browser, Agent,
Understanding, or Process card opens its full view. The Browser card's
`[ remote ]` action queues the same loopback-only `browser remote-view open`
operation as the command bar. Enter `inbox` from any view to return to Overview. `notify on`
enables a deduplicated terminal bell for newly observed needs-attention items;
notifications are off by default and contain no event payload.

Enter `tap` in Browser to replace fragile pixel targeting with up to nine numbered,
revision-bound semantic targets. Type or tap the number to execute through the
normal verified browser action path. `Esc` closes the overlay.

The phone layout remains semantic-first: continuous pixels are off by default,
which saves Chrome encoding, network bandwidth, and terminal redraw work.
`screenshot PATH` remains an explicit evidence capture. Enable an ephemeral
terminal-native view when visual feedback is useful:

```console
glass --tui-layout mobile --tui-live on --tui-live-quality data
```

Or enter `live on` in the command bar. Glass switches to Browser, starts a bounded
PNG screencast, and never persists those live pixels. Returning to Overview
suspends capture. Glass retains only the newest bounded PNG for the preview and
decodes one small ANSI canvas on demand, including when the focused renderer was
Herdr or Kitty; leaving Overview releases the decoded canvas while retaining the
newest bounded source frame for a later return. Disabling live or changing
backend releases both. Useful commands are `live status`, `live doctor`, `live
off|auto|on`, `live backend auto|herdr|kitty|ansi`, `live quality
auto|data|balanced|smooth`, and `live fit contain|cover|actual`.

`live quality auto` starts balanced, degrades under sustained frame drops, and
recovers after stable delivery. Capture is suspended when Browser is hidden.

Fit controls apply to ANSI sampling. Herdr and direct Kitty use aspect-safe
contain placement so their native overlays and browser pointer geometry remain
aligned.

`auto` only enables a native Herdr or Kitty renderer that Glass can detect.
`on` additionally permits the portable true-color ANSI half-block renderer.
Local balanced and smooth target 30 and 60 FPS. Verified fast remote links use
20/24/30 FPS; constrained or unknown remote links use 3/6/12. Auto quality
reduces capture scale before rate, settled/idle states throttle to 5/3 FPS,
and background acquisition pauses. The Overview thumbnail does not keep
background acquisition alive. Supply measured evidence or explicit
overrides when auto detection cannot know:

```console
glass --tui-transport remote-fast --tui-rtt-ms 35 \
  --tui-throughput-mbps 80 --tui-graphics kitty
```

Graphics requires an active probe or explicit configuration; terminal names
alone are not evidence. Mosh is semantic-only because it synchronizes cell
state rather than graphics-protocol state.

## Preserve work with Herdr

[Herdr](https://herdr.dev/docs/how-to-work/) is the recommended multiplexer for
Glass development sessions. Like tmux it owns persistent PTYs and supports
detach/reattach, but it also exposes agent state and has its own responsive
phone switcher. Glass detects `HERDR_ENV=1` as a multiplexer signal. Herdr,
shell transport, layout, and graphics remain independent in diagnostics.

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

Glass also writes a bounded reconnect capsule on clean TUI exit and restores
the mobile view and scroll position, browser target metadata/revision, and live
preferences on the next start. It does not persist composer drafts, command
history, palette queries, clipboard contents, or pixels. `capsule
save|show|clear` manages it explicitly. Running PTYs remain
owned by the resident process or Herdr; a capsule never pretends a process
survived a machine or Glass process crash.

Herdr can also own Glass's live image layer. Enable Herdr's experimental Kitty
graphics support in its configuration, then start Glass with `--tui-live auto`
or `--tui-live-backend herdr`. Glass connects to the pane-local
`HERDR_SOCKET_PATH`, opens one owned `pane.graphics.stream`, and sends raw PNG
frames on a one-frame queue. Detaching does not write image escape sequences
into shell history, and closing Glass releases the Herdr-owned layer. If the
stream fails, Glass falls back to direct Kitty when detected, then ANSI for
`live on`, and finally the semantic view.

## Recover the browser without leaving the TUI

A browser startup failure or disconnect becomes a Browser attention card; it
does not stop files, PTYs, the agent, or project state. The same printable
commands work in phone, compact, and wide layouts:

```text
browser status
browser reconnect
browser launch --port auto --headless
browser launch --port 9333 --headed --profile work
browser launch --incognito --chrome-path /opt/chrome
browser launch --chrome-path auto
browser targets 9222
browser attach --port 9222 2
browser target 2
browser semantic-only
```

`browser targets [PORT]` performs a fresh bounded probe and shows title,
origin, type, and target ID without retaining full URLs. Attach performs
another probe immediately before use and refuses unrelated or unknown
listeners. A number selects from the latest target list; an explicit ID is
also accepted. `--chrome-path auto` restores detected-browser selection.
`--incognito` and `--profile` are mutually exclusive in one launch request.
`browser auto-port`, `browser retry`, `browser connect`, and `browser
disconnect` remain compatibility aliases.

On every connection transition Glass clears target, visual, and semantic
revision state. A successful Ready transition performs a fresh observation
before browser tools become current again.

## Open the full application in Safari

There are two loopback-only Safari workflows:

- `safari` forwards the application server and is the simplest stable path;
- `browser remote-view open` serves the current Glass BrowserSession frames
  and revision-bound input without exposing CDP.

For Remote View, enter `browser remote-view open`. Glass prints a random-token
loopback URL and an `ssh -L` hint. Forward that port in the iOS SSH app, open
the printed URL in Safari, and keep the tunnel connected. The service accepts
at most four clients, retains only the newest frame, bounds input, and rejects
input whose displayed browser revision is stale. `browser remote-view close`
revokes the token and clients. The existing browser worker owns it; it never
launches a second browser.

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
  It therefore remains semantic-only; use Safari over a separate SSH tunnel
  for full fidelity.
- tmux and other multiplexers work with ANSI. Direct Kitty passthrough depends
  on their configuration, so `live doctor` reports what Glass actually chose.
- iOS terminal Smart Keys can still send arrows, `Esc`, and control keys, but
  Glass phone navigation does not depend on their arrangement.
- Mouse or touch events are optional. Glass enables terminal mouse capture while
  the TUI owns the screen and disables it during cleanup. The numbered navigation
  row and Overview cards accept clicks when the client forwards mouse events;
  every destination remains available through printable keys or commands.
- Orientation changes are handled through terminal resize events. The layout
  switches between phone, compact, and wide presentations in automatic mode.
- Use a UTF-8 locale. Glass uses Unicode state markers but never relies on
  color alone for meaning.

Run the browser-free renderer benchmark on the remote machine with:

```console
GLASS_LIVE_BENCH_ITERATIONS=100 \
  cargo run -p glass-browser --release --example terminal_live_benchmark
```
