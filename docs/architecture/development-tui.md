# Glass Dev TUI

Status: Current 0.3.13 source behavior (current-source changes are included here, not as a published-release claim).

This document is the architecture contract for the `glass`/Glass Dev terminal
workspace. The standalone browser-only TUI is a different product and reducer;
see [Glass terminal UI](tui.md). The private HTTP cockpit is a presentation of
the same development workspace, not another TUI authority; see
[Remote development cockpit](mobile-cockpit.md).

## Ownership and event flow

```text
Crossterm press/paste/mouse/resize/focus
                 |
                 v
        Glass Dev event loop
   (modal precedence + reducer calls)
                 |
        +--------+---------+
        |                  |
        v                  v
 DevTuiState          SnapshotWorker
 local focus,         refresh/tool/screenshot jobs
 cursors, drafts,     (one resident workspace handle)
 modals, selection           |
        |                    v
        +------------ DisplaySnapshot
                     (latest immutable projection)
                              |
                              v
                       Ratatui renderer
```

`DevTuiState` owns surface selection, responsive class, per-surface scroll,
command and composer input/cursors, modal state, editor cursor/selection and
scroll projection, browser workspace controller, and the latest display fields.
`SnapshotWorker` owns expensive refresh passes and governed tool/screenshot
jobs. It publishes the latest versioned `DisplaySnapshot`; rendering never
waits for a refresh. UI callbacks use `try_lock`; a workspace lock held by an
actor becomes a visible wait/error status rather than a terminal freeze.

The worker requests are `Refresh`, `RefreshConversation`, `Tool`,
`Screenshot`, and `ShutDown`. Refresh covers file listing, Git, agent history,
processes, tests and the other resident projections. Conversation refresh is a
cheap high-frequency tail pass. At most one coalesced refresh/conversation or
visual request is pending; tool and screenshot results return by bounded
channels. A worker job is not performed by the input loop.

Startup uses `open_for_tui`: the first frame is an immediate cockpit with guided
empty fields, then the worker hydrates projections. The synchronous `open`
helper remains for non-interactive callers. Dropping the worker requests
shutdown and joins when no bounded job is still running; active bounded work
must not delay terminal restoration.

## Surfaces, layouts, and state ownership

The shared workspace remains the authority for project buffers, actors,
processes, tasks, Git, browser, agents, and revisions. Switching a surface
only changes the projection; it does not create a second owner.

```text
Desktop (body: navigation | surface | context)
┌──────────────────────────────────────────────────────────────┐
│ header: GLASS DEV · surface · root · trust/mode        2 rows│
├───────────────┬───────────────────────────┬──────────────────┤
│ SURFACES 24   │ active surface (55%)      │ context ≥30      │
├───────────────┴───────────────────────────┴──────────────────┤
│ status/footer: status (2 rows; composer makes it 3)          │
└──────────────────────────────────────────────────────────────┘

Compact (body: navigation | surface)
┌──────────────────────────────────────────────────────────────┐
│ header 2 rows (3 with composer)                              │
├──────────────────────┬───────────────────────────────────────┤
│ SURFACES 22          │ active surface, minimum 36             │
├──────────────────────┴───────────────────────────────────────┤
│ status/footer 2 rows (3 with composer)                       │
└──────────────────────────────────────────────────────────────┘

Phone (single responsive pane)
┌────────────────────────────────────────┐
│ header 2 rows                          │
├────────────────────────────────────────┤
│ active surface, minimum 5 rows         │
├────────────────────────────────────────┤
│ status 2 rows, or input + status 3     │
└────────────────────────────────────────┘
```

Auto layout selects Phone below 72 columns **or** 22 rows, Compact below 118
columns **or** 32 rows, and Desktop otherwise. `--tui-layout desktop`,
`compact`, or `mobile` forces a class. Phone is geometry-responsive, not a
separate touch authority. Desktop and Compact expose the same state with
fewer columns; Compact removes the context column. Phone always stacks inner
workbench panes. Compact stacks them only when the surface pane is narrower
than 70 columns. Desktop keeps workbenches side-by-side. Content is bounded
and truncated rather than allowed to grow the terminal. Each `DevSurface` has
independent scroll.

### Responsive footer guidance

The normal Desktop/Compact footer is allocated two rows but renders one status
line inside its bordered area. Opening the Agent composer makes it three rows and
renders an input line followed by a status line. Opening the command palette
also uses the three-row allocation; its editable command/filter line carries
the selection or navigation hint. Phone uses the same two/three-row allocation:
normal status, or composer/palette input plus status. Full-screen editor help is
shortened below 70 columns and reduced to `Arrows · Alt-W · Ctrl-S` below 40
columns; the exit-protection line remains visible. Footer copy is guidance, not
a second keybinding or state owner.

| Surface | Authoritative projection | Guided empty/loading behavior |
|---|---|---|
| Trust | trust label and exact inspection items | starts here when project trust is required; `I` inspect, `O` open untrusted, `1` trust once, `T` trust project |
| Agent | readiness, conversation bubbles, event/tool cards, approval | `START HERE` when Pi is ready; `SETUP` directs to `:actions` when Pi is unavailable |
| Code | bounded files, focused buffer, cursor/dirty state, review comments/proposals/checkpoints, LSP | no files/file open messages direct to selection and Enter; no diagnostics is a valid state |
| App | embedded `BrowserWorkspaceController`: connection, target, semantic entities, workflow, visual path | detached/no page/loading/failed/recovery/semantic-only states stay visible |
| Terminal | managed process rows with health, PID, command and detected URL | `s` starts the detected suite; `a` opens actions; no process is an empty state |
| Tasks | workspace-local Agent checklist plus overnight DAG rows | empty todos direct to Plan accept or `glass.todo.write`; empty DAG directs to `a` or `:task create TITLE PROMPT` |
| Git | branch/change rows, selected file and inline diff | opening Git loads the selected diff; loading is visible and off-thread |
| Debug | debugger sessions and test evidence | no sessions is one start panel (`:debug start NAME COMMAND`) |
| More | Pi/readiness, kernels, experiments/replay, harnesses and routes | Enter runs the selected route; `doctor` stays on More |

## Navigation and modal routing

The event loop routes the strongest guard first: quit confirmation, editor exit
prompt, Ctrl-C, help, command-center menu, browser target picker/recovery,
agent approval, mutation confirmation, full-screen editor, file/session
pickers, composer dock, command palette, then ordinary surface input including
Git workbench keys. `Esc` closes the active modal/overlay; it does not cancel a
running worker job. Git keys stay live while a diff is open, but they do not
trap confirm or palette input.

| Input | Route and behavior |
|---|---|
| `1`–`8` (desktop/compact) | Agent, Code, App, Terminal, Tasks, Git, Debug, More |
| `1`–`5` (Phone) | Agent, Code, App, Tasks, More |
| `Tab` / `Shift-Tab` | next/previous primary surface |
| Left/Right | previous/next surface (unless Alt-modified browser history) |
| Up/Down, `j`/`k` | focused list movement; otherwise the current surface scrolls |
| PageUp/PageDown, Home/End | surface scroll/page or bounds |
| `a` outside Agent | open the current surface command center; `:` opens the filtered palette |
| `Ctrl-L` | open the shared composer dock on the current surface |
| `Ctrl-Shift-A` | cycle Ask, Plan, and Agent; default Agent; Ask/Plan fail closed for mutations |
| `?` | help; `j`/`k`, PageUp/PageDown and Home/End scroll; `?`/Esc closes |
| `Enter` on Agent | start/continue the agent interaction; with text focus, submit composer |
| `Enter` on Code | open the selected file for full-screen edit; preview queues if the workspace is busy |
| `[` / `]` | cycle Agent session or Code buffer |
| `T` on App | queue browser target picker; App Enter activates selected semantic entity |
| `Alt-Left` / `Alt-Right` on App | guarded browser Back/Forward |
| `Ctrl-R` on App | guarded browser reload |
| `d` / Git enter | queue selected/full diff in the worker; opening Git loads the selected file |
| Space on Git | stage or unstage the selected file |
| `c` on Git | open `git commit` in the palette |
| `s` on Terminal | queue the project-detected development suite |
| `Y`/Enter or `N`/Esc | approve/deny the one frozen mutation confirmation or agent approval |
| `Ctrl-C` | open Glass quit confirmation, including from the editor; confirmed quit restores terminal |
| `Esc` | close current modal, palette, diff, recovery, or return INSERT→NORMAL then leave a clean editor |
| mouse left-click | select a navigation tab or the dock; double-click opens; right-click/long-press opens actions; wheel scrolls |
| paste/focus/resize | accepted while terminal modes are enabled; focus loss closes browser overlays |

The command center lists actions for the current surface plus Search commands and
Quit. Search accepts a typed route, including expert commands such as
`help`/`quit`; action placeholders are prefilled without `NAME`, `PATH`, or
other argument tokens. Browser action commands are delegated to the embedded
browser workspace and preserve its revision checks.

## First launch and agent composer

On construction Glass reads workspace trust and Pi readiness without blocking on
the full projection. A trust-required project starts on Trust. Otherwise Agent
is selected: a ready Pi shows `Ready · describe a coding task`, while an
unconfigured Pi shows `Pi setup required · press :actions or Enter to continue`.
The `:actions` routes are `agent setup`, `agent update`, `agent setup login`, and
`agent doctor`; login temporarily hands the terminal to Pi and resumes Glass.
`--yolo` is process-scoped unrestricted development mode and is shown in the
header; it does not create a second workspace.

The shared composer dock is a local draft with a character cursor. `Ctrl-L`
opens it on any surface. `i`, Enter on the Agent surface, or typing a
non-digit Agent character also opens it. Default mode is Agent. `Ctrl-Shift-A`
cycles Ask, Plan, and Agent; `/ask`, `/plan`, `/agent`, and `/todo` also set
the mode. Ask inspects only. Plan writes a bounded numbered plan. Only Agent
may mutate. `Enter` submits immediately and keeps the composer open; submitted
text is rendered optimistically, while the worker/event stream appends
assistant deltas and tool activity. `Ctrl-D` toggles steer mode for an active
turn; `Ctrl-X` aborts the selected agent. `Ctrl-A/E/U/W` move to start/end,
clear, or delete a word; Left/Right and Backspace edit the draft. Bracketed
paste is inserted as one bounded edit. A send is a governed background request
and never executes from the input loop. If another job is active, the draft is
retained and the send waits; a failed transport marks the pending message
failed and restores its draft for edit-and-retry. Workspace-local todos
(`glass.todo.*`) persist at `.glass/todos/session.json` and form the Agent
checklist shown on Agent and Tasks; they are not the overnight task DAG.

## Code projection and editor

Code navigation and full-screen editing are separate modes. Code shows a
bounded file list, syntax-aware source preview, review summary and diagnostics.
Selecting a file opens/refreshes a shared `EditorBuffer`; the full-screen editor
projects the same buffer, actor, dirty bit, cursor and selection. Comments are
anchored to line ranges; proposals carry base hash/revision and can become
accepted, rejected, or stale; checkpoints are named project-scoped persisted snapshots that restore in-memory buffers without writing disk. `Alt-A`
prepares the focused buffer and current cursor/selection for an Agent prompt;
the attachment includes unsaved text and is bounded, with the agent able to
request the remainder through file tools.

Editor collaboration actions are command-palette routes on the Code surface:
`editor comment-selection TEXT` (or `editor comment PATH START END TEXT`),
`editor comment-resolve ID`, `editor replace-selection TEXT`,
`editor propose PATH SUMMARY TEXT`, `editor proposals`,
`editor accept ID`/`editor reject ID`, `editor checkpoint NAME`,
`editor checkpoints`, and `editor restore CHECKPOINT_ID`. Mutating routes are
queued through the same governed confirmation and revision context as agent
tools; read routes refresh the Code projection. A proposal whose base hash or
revision no longer matches is stale rather than silently applied.

```text
source buffer (one-based line/column)
        │ shared projection
        ├── Code preview + files/review/LSP
        └── full-screen editor
             │
             ├── cursor + optional anchored selection
             ├── dirty/save/undo/redo
             └── scroll (line + column)
```

`Ctrl-S` saves; `Ctrl-Z`/`Ctrl-Y` undo/redo; arrows move and Shift+arrows
extend the selection; Enter inserts a newline and Backspace deletes. `Alt-W`
toggles soft-wrap. No-wrap is the default: source columns are preserved and
horizontal scroll follows the cursor. Soft-wrap reflows each source line to the
editor inner width, repeats a blank gutter on continuation rows, maps the
one-based cursor to its visual row/column, and disables horizontal scrolling.
Changing modes resets scroll and re-runs cursor visibility. Cursor visibility
is kept synchronized after edits, movement, resize, projection refresh and
selection changes.

### Native editor physics and proof

The full-screen editor has `Normal`, `Insert`, `Select`, and `Agent` modes;
new buffers start in `Insert`. Normal mode supplies `hjkl`, word and line
motions, `%`, `gg`/`G`, find/till motions, operators (`d`, `c`, `y`), and
structural textobjects. An incremental tree-sitter cache supplies supported
function, class, pair, argument, parameter, field, string, and comment ranges;
lexical word/pair selection remains the fallback when parsing is unavailable.
`gm`/`gn` can add matching structural selections.

The editor can show a local or configured resident-Pi fill-in-the-middle ghost.
`Tab` accepts the full ghost and `Ctrl-Right` accepts its next word; acceptance
advances to the next incomplete site when one exists. Agent pair-apply streams
a bounded proposal over the buffer: `Enter` accepts, `n` rejects, `Esc` yields,
and typing yields back to Insert. Hunk navigation uses `[`/`]`; `Enter` accepts
the selected hunk and `n` rejects it.

Resident LSP hover (`K`), definition (`gd`), references (`gr`), symbols (`o`),
and inlay hints stay attached to the focused buffer. Gutter marks distinguish
LSP diagnostics, Git hunks, Agent carets, source-page links, proof results, and
open comments. A composer request containing a supported prove/verify/expect
clause attaches a browser predicate to `glass.agent.send`; the resulting live
`glass.browser.verify` evidence drives the proof card and gutter mark. Text
that merely contains those words is not proof until the browser result passes.

The editor starts in INSERT. `Esc` returns to NORMAL. `Esc` from NORMAL on a
clean buffer leaves the editor. An unsaved buffer offers `S` save and leave,
`D` discard and leave, `Q` discard and quit Glass, or Esc stay. Save/discard
errors remain in the prompt with retry guidance. `Ctrl-C` does not take the
editor-exit path when editor input is active; it opens Glass quit confirmation.
If the unsaved-exit prompt is already open, its save/discard/stay choices retain
priority.

## Browser visual plane and dev-suite actions

App embeds the canonical browser workspace with adapter kind
`EmbeddedDevelopment`; standalone Browser TUI uses `Standalone` and must not be
mixed with this surface. The App visual plane keeps semantic inspection live
while optional pixels are displayed. `:browser start`, `:browser navigate URL`,
`:browser observe`, `:browser targets`, and guarded type/activate actions go
through `SnapshotWorker` tool requests. Target selection and actions carry the
expected browser revision; stale references fail closed and require a fresh
observation. Browser start failures expose a recovery sheet: attach a compatible
endpoint, launch an isolated automatic port, retry the preferred port (where
applicable), or dismiss. Project and agent state survive browser recovery.

`browser view` (from the App action menu or palette) toggles live presentation;
there is no standalone `v` toggle in the Glass Dev reducer (`v` selects App in
wide navigation). The selected path is Herdr, Kitty, or bounded ANSI when
available and allowed, otherwise Semantic-only with a visible reason. In App,
ANSI frames use a bounded `AnsiPane`; Herdr uses its latest-frame queue; Kitty
writes native graphics after Ratatui and clears/repositions on geometry change.
Live capture is off by default unless CLI live mode is On/Auto, and an
unavailable path clears the toggle instead of claiming success. Visual requests
are coalesced and do not block key handling.

Terminal process actions are governed and bounded: `s` queues the detected
project dev command, while palette routes can start a custom command, inspect
logs, stop/restart/remove, resize, send input, report health, and list detected
ports. Process rows distinguish starting/healthy/exited/stopped/failed; output
and URLs are bounded. Tasks, Git mutations, browser actions, editor proposals,
and agent tools retain exact authorization, generation, and project-revision
context; confirmations are one-use and denial is explicit.

## Rendering states and cleanup

Every surface preserves its prior projection while loading or busy and exposes
an actionable status. Empty means no data (not a hidden error); errors retain
previous data when safe and show recovery text; constrained terminals retain
header/footer and truncate panel content. App additionally distinguishes
Detached, Starting, Connected, Recovering, and Failed browser connection phases,
plus semantic-only and live presentation. Agent distinguishes setup, drafting,
sending, tool-running, approval, failed-send and settled conversation states.
Git diff loading is visibly `Loading Git diff…`; browser observation and live
screenshots are similarly marked pending.

`TerminalGuard` enables raw mode, alternate screen, mouse capture, focus
reporting, and bracketed paste. Its Drop path disables all of them, leaves the
alternate screen, and shows the cursor. The same cleanup is used after normal
quit, confirmed Ctrl-C quit, startup failure, and external-harness handoff;
external harnesses suspend these modes, run in the real terminal, then clear and
redraw on resume. Kitty graphics receive an explicit clear on shutdown or pane
change. Cleanup never destroys the shared project/workspace state.

## Source of truth

The implementation contracts are in
[`tui/state.rs`](../../crates/glass-dev/src/tui/state.rs),
[`tui/mod.rs`](../../crates/glass-dev/src/tui/mod.rs),
[`tui/render.rs`](../../crates/glass-dev/src/tui/render.rs),
[`tui/snapshot.rs`](../../crates/glass-dev/src/tui/snapshot.rs),
[`tui/file_view.rs`](../../crates/glass-dev/src/tui/file_view.rs),
[`tui/editor.rs`](../../crates/glass-dev/src/tui/editor.rs),
[`tui/parse.rs`](../../crates/glass-dev/src/tui/parse.rs),
[`fim.rs`](../../crates/glass-dev/src/fim.rs), and
[`development/editor.rs`](../../crates/glass-dev/src/development/editor.rs).
The shared browser contract is
[`browser_workspace/mod.rs`](../../crates/glass-browser/src/browser_workspace/mod.rs).
