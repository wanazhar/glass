# Development workspace TUI layout

Status: Local-only 0.3.2 release-candidate contract

The Development mode is a native Ratatui view layered beside the existing
Browser Workspace. It does not replace browser authority or make screenshots
the default context.

```text
┌─ Glass — Development ─ project / branch ─────────────────────────────┐
│ Activity          │ Files       │ Editor / selected source │ Runtime   │
│ events            │ ▾ src       │  1 │ fn main()            │ ACTORS    │
│ diagnostics       │   main.rs   │  2 │                     │ ◆ Human   │
│ agent stream      │   lib.rs    │                         │ ◆ Agent   │
│                   │             ├─────────────────────────┤ PROCESSES │
│                   │             │ impact / diff / timeline │ → dev    │
├───────────────────┴─────────────┴─────────────────────────┴───────────┤
│ project ... | status | browser revision | PTY | Enter command          │
└────────────────────────────────────────────────────────────────────────┘
```

## Interactions

| Input | Scope | Behavior |
|---|---|---|
| `F7` | global | enter Development mode |
| `F1`–`F6` | global | return to the existing browser surfaces |
| `project` | command area | show detected project configuration |
| `project open PATH` | command area | open a bounded native buffer |
| `project edit PATH CONTENT` | command area | save an actor-attributed buffer edit |
| `project run NAME COMMAND` | command area | start a managed PTY process |
| `project processes` | command area | inspect bounded process state |
| `project agent PROMPT` | command area | stream local harness events and tool results |
| `Esc` | busy/error | cancel browser work or dismiss an error |

## State variants

- **Loading**: project panes show `Project detection pending` while the
  browser worker connects independently.
- **Empty**: no open buffer or managed process is shown as an explicit empty
  state; the command hint remains visible.
- **Error**: bounded error text appears in the existing overlay and the event
  is retained in Activity.
- **Busy**: browser operations retain their existing cancellation and mutation
  lease behavior; PTY processes remain independently observable.
- **Constrained**: the Development view uses bounded file and source previews;
  the existing two-pane browser view remains available on smaller terminals.

The runtime panel distinguishes confirmed state (`◆`, `→`, `✓`) from inferred
source/runtime links. A link never becomes certain merely because it is shown
in the TUI.
