# Glass Dev product workspace

Status: Current 0.3.12 source behavior

Glass Dev presents user work rather than its internal service registry. The
desktop navigation has eight primary destinations:

```text
Agent · Code · App · Terminal · Tasks · Git · Debug · More
```

Kernels, experiments, replay, daemon/workspace status, customization, and
trust inspection live under `More` or the command palette. Phone has five
direct destinations:

```text
Agent · Code · App · Tasks · More
```

The same `DevelopmentWorkspace` remains authoritative. Changing destinations
does not create another editor, process, task, browser, or agent owner.

## Interaction

| Input | Behavior |
|---|---|
| `1`-`8` desktop | Agent, Code, App, Terminal, Tasks, Git, Debug, More |
| `1`-`5` phone | Agent, Code, App, Tasks, More |
| `Tab` / `Shift-Tab` | move between product destinations |
| `j` / `k`, arrows, wheel | scroll the focused pane; App `j`/`k` moves semantic selection |
| `i` in Agent | open the ordinary no-ID conversation composer |
| `:` | open a cursor-editable fuzzy action palette scoped to the active surface; type a command directly for expert routes such as `help`, `quit`, or `view` |
| `Y`/Enter or `N`/Esc | approve one frozen mutation or deny it |
| `H` / `G` in App | take human control / reconcile and return Glass control |
| `?` | open keyboard help; `j`/`k` and PageUp/PageDown scroll help |
| `Esc` | close the active modal, editor, palette, diff, recovery sheet, or confirmation |
| `Ctrl-C` | restore the terminal and quit immediately, including during background work |
| `d` in Git | queue the diff load off-thread and open the inline diff when ready |
| `v` in App | toggle bounded ANSI live view; failure clears the toggle and reports the reason |
| mouse, paste, focus, resize | processed while their terminal modes are enabled |

The Agent composer creates or addresses the current assistant session without
requiring an agent ID. `Enter` submits immediately, keeps the composer open,
and renders the submitted text optimistically while the resident event stream
adds assistant deltas and tool activity. The send, steer, follow-up, and abort
requests use the governed background actor path; the input loop never performs
the agent broker operation itself. If an App entity is selected, the composer
supplies its reference and visible revision as bounded context and transfers
mutation ownership to the agent. Human takeover pauses agent browser mutation
until reconciliation. If another background job is active, submitting keeps
the composer draft instead of dropping it; transport failures keep the draft
and expose an edit-and-retry state.

## Designed projections

- Agent shows readiness, conversation, tool/event cards, and confirmation.
- Code shows line-numbered buffers, cursor/dirty/actor state and diagnostics.
- App renders the canonical `BrowserWorkspace` semantic selection and workflow.
- Terminal shows named PTYs, health, PID, transport and detected URL.
- Tasks use `✓ verified`, `◇ settled`, `× failed`, `! blocked`, and `● active`.
- Git shows branch/ahead-behind and change rows; discard, commit and push require
  the frozen confirmation sheet.
- Debug shows sessions, processes, breakpoints, watches, tests and source state.

The Git workflow also exposes `github status` and `github review` from the
command palette. `github ship TITLE [--draft]` creates a pull request only
after the same one-use confirmation; GitHub authentication and origin checks
remain explicit.

## Code and diff rendering

Code and inline Git diffs choose syntax from the path, not from file contents.
The renderer uses bundled syntect grammars where available and keeps a
deterministic manual/plain fallback when a path has no grammar. TypeScript
(`.ts`, `.tsx`, `.mts`, `.cts`) uses the JavaScript grammar; Swift and Dart
use the C++ grammar; Kotlin (`.kt`, `.kts`) uses the Java grammar; and
Dockerfile-like names use the shell grammar.

Markdown files render headings, lists, links, and inline code, and highlight
fenced blocks delimited by backticks or tildes. Fence language tokens support
the same TypeScript, Swift, Kotlin, and Dart aliases. Mermaid files (`.mmd` or
`.mermaid`) and supported `flowchart`, `graph`, and `sequenceDiagram` sources
render deterministic terminal diagrams; unsupported forms remain styled source.
Diffs track each file path from its `---`/`+++` headers, show old/new line
numbers and add/remove backgrounds, then apply the path grammar to changed
content.

Raw JSON is not a default product surface. Governed Inspect/export tools remain
available for expert diagnosis.

## Execution, responsiveness, and cleanup

`SnapshotWorker` owns refresh, conversation, browser screenshot, and governed
tool jobs. It publishes a versioned `DisplaySnapshot`; the renderer consumes
resident projections without locking the workspace. Workspace access from
input callbacks uses a non-blocking lock attempt, so an actor-held browser,
Git, process, or project operation reports a wait state instead of freezing the
terminal. Startup renders an initial cockpit before the first full projection
pass; the worker hydrates the remaining surfaces immediately after.

Confirmed mutations retain their exact serialized call and revision context.
Agent setup, browser recovery, Git diff, editor-adjacent reads, and agent
composer actions use the same worker boundary. Active bounded jobs do not delay
terminal restoration during shutdown.

## Responsive behavior and cleanup

Auto layout uses phone below 72 columns or 22 rows, compact below 118 columns
or 32 rows, and desktop otherwise. Explicit layout overrides remain available.
Each destination owns independent scroll state. Phone tests exercise 48x18,
64x24 and 80x24 flows.

The terminal guard enables and later disables raw mode, alternate screen,
mouse capture, focus reporting, and bracketed paste. Idle workers are joined at
their lifecycle boundary; active bounded jobs are allowed to finish or detach
without delaying terminal restoration. A browser recovery sheet never destroys
project state. Phone mode is a geometry-responsive single-pane layout, not a
touch-specific authority path.
