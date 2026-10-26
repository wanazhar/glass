# Core command presentation in Glass

Glass routes commands from the resident Core catalogue into workspace-owned
surfaces. Core remains the authority for the command catalogue, execution, and
the meaning of each result. Glass presents a command result in its owner-scoped
command panel; palette discovery, status, provider settings, and task state do
not become conversation messages. Actual assistant turns and attributed agent
tool results remain in the conversation.

## Versioned Core contract

`commands/list` advertises `commandPresentationVersion: 1`, and each command
declares a `presentationKind`. A `commands/execute` result carries a
`presentation` object with `version`, `surface`, `kind`, `title`, `command`, a
structured text `content` object, and supported panel actions. These fields are
additive to the existing `text`, `message`, and result-specific fields so the
standalone Core CLI and older clients keep their existing behavior.

Glass accepts version 1 panel results. If an older runtime omits the typed
object, Glass uses the `coreCommand` operation boundary and the named result to
show a generic panel. Unknown future presentation versions also fall back to a
generic panel. Command errors carry their request name through the host bridge
and stay in the matching panel; if an older bridge omits the name, the pending
panel supplies it. A closed panel is not reopened by a late response.

## Core catalogue disposition

Core publishes the complete built-in catalogue with a presentation disposition
for each command. The policy is generated alongside the catalogue and tested
against every current built-in entry:

| Core command class | Glass destination | Effect |
|---|---|---|
| `help` | Searchable command panel | Shows the Core catalogue |
| `status` | Command panel | Shows current Core session status |
| `compact` | Command panel | Shows the compaction receipt after Core completes it |
| `model` | Glass provider/model picker | Loads the Core model catalogue; selection applies to the current Core session |
| `settings` | Glass Core settings and model panel | Loads the selected session settings and Core model catalogue; selecting a model applies through Core's session configuration API |
| Other interactive Core commands | Unsupported command panel | Explains that this workspace has no control and confirms that no action ran |

The classification comes from Core's catalogue and typed result contract, not
from matching output text. Glass does not switch to the standalone Core TUI
automatically. People can explicitly use the shell when a command has no Glass
control.

## Glass-owned palette actions

Glass-local actions are declared by the per-surface `SurfaceAction` catalogue
and dispatched through the shared `command::execute` router. Their output stays
with the owning Glass surface: editor, browser, Git, tasks, debugger, settings,
or a concise status notification. The action handler and its effect metadata
remain the source of truth; a local action does not create a synthetic Core
conversation message.

## Verification

The Core runtime tests require every built-in command to have one recognized
presentation disposition. TUI projection tests cover catalogue, status,
result, picker, unsupported, and error cases, including older results without
presentation metadata. State tests verify owner scoping, hidden command output,
error placement, unsupported-command wording, and late-response behavior. The
opt-in launcher PTY acceptance test checks panel navigation and draft return at
phone, compact desktop, and wide desktop sizes.
