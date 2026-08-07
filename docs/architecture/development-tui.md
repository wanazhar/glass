# Development workspace TUI layout

Status: Implemented 0.3.2 contract

The Development mode is a native Ratatui view layered beside the existing
Browser Workspace. It does not replace browser authority or make screenshots
the default context.

```text
┌─ Glass — Development ─ project / branch ───────────────────────────────┐
│ FILES / GIT │ NATIVE EDITOR                         │ RUNTIME / TESTS   │
│ ▾ src       │  1 │ fn main() {                     │ ● dev healthy     │
│  M main.rs  │  2 │   checkout();                   │ ✓ unit 81/81      │
│  ! lib.rs 2 │                                      │ ACTORS             │
│              ├─ LIVE APP / SEMANTICS ───────────────┤ ◆ Human            │
│              │ action.checkout.submit · rev 582     │ ◆ Glass Agent      │
├──────────────┴───────────────────────────────────────┴───────────────────┤
│ AGENT / ATTRIBUTED TIMELINE                                             │
├──────────────────────────────────────────────────────────────────────────┤
│ project ... | browser revision | PTY health | Enter command              │
└──────────────────────────────────────────────────────────────────────────┘
```

## Interactions

| Input | Scope | Behavior |
|---|---|---|
| `F7` | global | enter Development mode |
| `F1`–`F6` | global | return to the existing browser surfaces |
| `project` | command area | show detected project configuration |
| `project open PATH` | command area | open a bounded native buffer |
| arrows/type/`Ctrl-S` | editor | navigate, edit, and atomically save |
| `Ctrl-Z` / `Ctrl-Y` | editor | bounded undo and redo |
| mouse click | editor/live app | place editor cursor or route browser input |
| `project search QUERY` | command area | fuzzy-search the development model |
| `project edit PATH CONTENT` | command area | save an actor-attributed buffer edit |
| `project run NAME COMMAND` | command area | start a managed PTY process |
| `project processes` | command area | inspect bounded process state |
| `project agent PROMPT` | command area | stream local harness events and tool results |
| `project pi ACTION` | command area | queue a real Pi RPC request without blocking input |
| `project diagnostics PATH` | command area | run LSP work off the input/render loop |
| `Esc` | busy/error | cancel browser work or dismiss an error |

## State variants

- **Loading**: project panes show `Project detection pending` while the
  browser worker connects independently.
- **Empty**: no open buffer or managed process is shown as an explicit empty
  state; the command hint remains visible.
- **Error**: bounded error text appears in the existing overlay and the event
  is retained in Activity.
- **Busy**: browser operations retain cancellation and mutation leases. LSP
  and Pi work run on bounded worker channels, so editor, render, and browser
  input continue to be polled.
- **Constrained**: the Development view uses bounded file and source previews;
  the existing two-pane browser view remains available on smaller terminals.

The runtime panel distinguishes confirmed state (`◆`, `→`, `✓`) from inferred
source/runtime links. A link never becomes certain merely because it is shown
in the TUI.

The Development layout assigns 21% to the file/Git surface, 54% to the editor
and live app, and 25% to runtime/tests/actors. The lower 24% is the agent and
attributed timeline. On constrained terminals, content stays bounded and the
browser-only modes remain one key away. The browser graphical geometry is
recomputed against the live-app rectangle, never the editor rectangle.
