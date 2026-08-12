# Glass Dev product workspace

Status: Implemented 0.3.6 contract

Glass Dev presents user work rather than its internal service registry. The
desktop navigation has seven primary destinations:

```text
Agent · Code · App · Terminal · Tasks · Git · Debug
```

Kernels, experiments, replay, daemon/workspace status, customization, and
trust inspection live under contextual `More` or the command palette. Phone
has five direct destinations:

```text
Agent · Code · App · Tasks · More
```

The same `DevelopmentWorkspace` remains authoritative. Changing destinations
does not create another editor, process, task, browser, or agent owner.

## Interaction

| Input | Behavior |
|---|---|
| `1`-`7` desktop | Agent, Code, App, Terminal, Tasks, Git, Debug |
| `1`-`5` phone | Agent, Code, App, Tasks, More |
| `Tab` / `Shift-Tab` | move between product destinations |
| `j` / `k`, arrows, wheel | scroll the focused pane; App moves semantic selection |
| `i` in Agent | open the ordinary no-ID conversation composer |
| `:` | open cursor-editable fuzzy command palette with bounded history |
| `Y`/Enter or `N`/Esc | approve one frozen mutation or deny it |
| `H` / `G` in App | take human control / reconcile and return Glass control |
| mouse, paste, focus, resize | processed while their terminal modes are enabled |

The Agent composer creates an ordinary assistant session when none exists and
otherwise addresses the selected/current conversation without requiring an
agent ID. Multi-agent IDs remain in orchestration details. If an App entity is
selected, the composer supplies its reference and visible revision as bounded
context and transfers mutation ownership to the agent. Human takeover pauses
agent browser mutation until reconciliation.

## Designed projections

- Agent shows readiness, conversation, tool/event cards, and confirmation.
- Code shows line-numbered buffers, cursor/dirty/actor state and diagnostics.
- App renders the canonical `BrowserWorkspace` semantic selection and workflow.
- Terminal shows named PTYs, health, PID, transport and detected URL.
- Tasks use `✓ verified`, `◇ settled`, `× failed`, `! blocked`, and `● active`.
- Git shows branch/ahead-behind and change rows; discard, commit and push require
  the frozen confirmation sheet.
- Debug shows sessions, processes, breakpoints, watches, tests and source state.

Raw JSON is not a default product surface. Governed Inspect/export tools remain
available for expert diagnosis.

## Responsive behavior and cleanup

Auto layout uses phone below 72 columns or 22 rows, compact below 118 columns
or 32 rows, and desktop otherwise. Explicit layout overrides remain available.
Each destination owns independent scroll state. Phone tests exercise 48x18,
64x24 and 80x24 flows.

The terminal guard enables and later disables raw mode, alternate screen,
mouse capture, focus reporting and bracketed paste. Browser, agent, process,
and daemon workers remain owned and are joined/reaped at their lifecycle
boundary; a browser recovery sheet never destroys project state.
