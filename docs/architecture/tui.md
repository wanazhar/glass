# Standalone Browser TUI

Status: Current standalone Browser TUI reference (Glass 0.3.13 source behavior).

This is the independently installable browser-only terminal product. It owns a
`BrowserSession` and a `BrowserWorkspaceController` with the `Standalone`
adapter. It does not own project files, editors, processes, tasks, agents,
Git, or debugger state. The Glass Dev product has a separate event loop,
`DevTuiState`, and `SnapshotWorker`; its contract is in
[Development TUI](development-tui.md).

## Ownership and flow

```text
Crossterm event
      |
      v
BrowserTui (one async app object)
  command · mode · status · page
  BrowserSession + Standalone workspace controller
      |                         |
      +-- awaited browser I/O   +-- local semantic reducer
      |                         |
      v                         v
  Browser backend/CDP       Ratatui frame
```

The standalone event loop awaits browser commands on its application object.
Semantic selection movement is local and does not send a CDP highlight request
for each arrow or wheel event. Browser actions are revision-guarded by the
shared workspace controller. Unlike Glass Dev, this TUI has no generic worker
boundary for every browser command; slow browser I/O can occupy its event-loop
task while awaited.

## Layout and state projection

The renderer always reserves a five-row bordered header, a bounded content area,
and a three-row bordered command footer. Width changes the content presentation
and title but not the ownership model.

```text
┌──────────────────────────────────────────────────────────────┐
│ GLASS BROWSER · class                                      5 │
│ connection · browser revision · presentation · owner · focus │
│ latest operation status                                      │
├──────────────────────────────────────────────────────────────┤
│ bounded semantic page / help / target list                  │
│ selected entity and workflow evidence; pixels only if opted  │
├──────────────────────────────────────────────────────────────┤
│ visual path · status · > command                            3 │
└──────────────────────────────────────────────────────────────┘
```

Phone is ≤72 columns, Compact is 73–109, and Desktop is ≥110. The current
renderer classifies by width; height constrains the content pane, which remains
at least five rows. Phone title is `GLASS BROWSER · PHONE`, Compact and Desktop
use their corresponding labels. Content is bounded before rendering. The
header keeps connection phase, browser revision, presentation path, input owner,
focus, and latest status visible in every class.

The controller owns these state dimensions: `Detached`, `Starting`, `Connected`,
`Recovering`, or `Failed` connection; selected semantic entity and revision;
target list; `Glass`, human, or agent input owner; semantic scroll and focus;
workflow text; and `Herdr`, Kitty, Sixel, ANSI, or Semantic-only presentation.
Disconnects and revision failures remain visible rather than erasing the prior
semantic evidence.

## Input routing

Input is keyboard-first. `Enter` with a non-empty command submits and awaits one
command; `Enter` with an empty command activates the selected semantic entity.
`Esc` clears the command and returns Help/Semantic mode to Browser mode, or
closes the current browser workspace overlay. `q` and Ctrl-C exit directly in
this standalone product (Glass Dev uses a quit confirmation).

| Input | Behavior |
|---|---|
| `l` with empty command | prefill `launch auto` and launch/recover on a free port |
| `a` with empty command | prefill `attach ` for a verified DevTools port |
| `n` with empty command | prefill `navigate ` for a URL/domain (`https://` optional) |
| `t` with empty command | prefill `type ` for selected semantic input |
| `Enter` with empty command | activate the selected semantic target |
| `Enter` with command | parse and asynchronously await one browser command |
| Up/Down or `j`/`k` with empty command | move local semantic selection; selection wraps/clamps per controller |
| mouse wheel | move local semantic selection; it is not browser pixel scrolling |
| left click in semantic rows | select the clicked bounded semantic entity |
| `Tab` / `Shift-Tab` | move workspace focus, including command/footer focus |
| `Alt-Left` / `Alt-Right` | guarded browser Back/Forward |
| `Ctrl-R` | guarded browser Reload |
| `?` with empty command | show help; Esc returns to Browser mode |
| `:` with empty command | announce command-line mode; typing fills the command buffer |
| Backspace | remove the final command character |
| paste | normalize CR/LF to spaces and cap insertion at 8,192 characters |
| `q` / Ctrl-C | close browser session and restore terminal |

Focus loss closes the current browser overlay. Key-release events are ignored;
only key presses enter the reducer. Browser commands that fail set a visible
error and mark the workspace disconnected where appropriate, with recovery
choices such as reconnect, `launch auto`, or `launch PORT`.

## Command surface

The prompt accepts the following commands without requiring raw JSON:

| Area | Commands |
|---|---|
| Lifecycle | `launch auto`, `launch PORT`, `attach PORT`, `reconnect`, `stop` |
| Navigation | `navigate URL`; Back/Forward/Reload/Stop-loading are guarded keyboard intents |
| Observation | `observe`, `semantic`, `screenshot`, `state` |
| Selection | `targets`, `select ID` |
| Interaction | `type TEXT`, `scroll PIXELS` |
| Workflow | `workflow list`, `workflow run FILE`, `workflow pause`, `workflow resume FILE`, `workflow cancel`, `workflow verify` |
| Presentation | `live on`, `live off` |
| Exit/help | `help`, `quit`, `exit`, `q` |

`observe` refreshes the bounded accessibility `PageContext`; `semantic`
selects the semantic presentation. Target discovery, frame selection, storage,
downloads/uploads, diagnostics, policy, certification, and extension
administration remain explicit CLI/MCP/library contracts rather than a second
unbounded prompt syntax.
`select ID` invalidates prior semantic references and requires fresh observation.
Actions such as the selected-target type carry the expected browser revision and
fail closed when stale. `stop` stops the browser but keeps the TUI workspace
object; reconnect/launch/attach restore a usable session explicitly. Back,
Forward, Reload, and Stop-loading are exposed as keyboard intents rather than
standalone command strings.

## Loading, empty, busy, and failure states

- **Loading:** retain the previous bounded page when available and show the
  operation in the header/footer.
- **Empty/detached:** show `No browser session` and the START HERE routes for
  launch, attach, navigate, and help.
- **Busy:** the event loop is awaiting the selected browser operation; status
  remains visible, but standalone commands do not provide a universal
  cancellation-token action. Use the browser's explicit stop/reconnect command
  after control returns.
- **Error/disconnected:** preserve semantic state where safe, expose the error,
  and guide reconnect, launch-auto, or explicit-port recovery.
- **Help:** replace content with the bounded command reference; Esc restores
  Browser mode.
- **Constrained:** preserve header/footer and truncate content to the available
  height; never send an unbounded DOM or screenshot into the pane.

## Live browser presentation

Live pixels are explicit and bounded. CLI policy selects Herdr when available,
otherwise Kitty/ANSI as configured, with Semantic-only when no allowed path is
usable. Live mode is disabled unless `--tui-live on` or `auto` requests it; the
`live on|off` command changes that state. ANSI decodes a bounded screenshot into
an `AnsiPane`; Herdr consumes latest-frame-only messages; Kitty emits native
graphics after the Ratatui pass and clears/repositions when geometry changes.
A failed capture visibly reports the failure and turns live mode off; it never
claims pixels are active. Hidden/non-Browser content is not rendered over.

Visual refresh uses the configured quality interval and fit policy. The semantic
page and status remain the authority even while a visual frame is pending. The
standalone app does not import Glass Dev's project/editor/agent snapshot worker.

## Terminal lifecycle

The TUI requires interactive stdin and stdout. `TerminalGuard` enables raw mode,
alternate screen, mouse capture, focus reporting, and bracketed paste. On q,
Ctrl-C, command `quit`/`exit`, normal return, or an error, it shuts down native
graphics, closes the owned browser session, disables those modes, leaves the
alternate screen, and shows the cursor. A browser session is closed by the
standalone app; unrelated project state is not touched.

## Source of truth

See [`tui/app.rs`](../../crates/glass-browser/src/tui/app.rs), the shared
[`browser_workspace/mod.rs`](../../crates/glass-browser/src/browser_workspace/mod.rs),
and the browser connection/presentation contracts in
[`connection.rs`](../../crates/glass-browser/src/connection.rs) and
[`terminal_graphics/mod.rs`](../../crates/glass-browser/src/terminal_graphics/mod.rs).
