# Development Runtime

Glass `0.3.2` ships two products from one workspace:

- `glass-browser` is the standalone browser control plane and Rust library;
- `glass-dev` installs `glass`, the terminal-native development environment
  that consumes `glass-browser`.

Install the full environment and check a project without learning the crate
layout:

```console
cargo install glass-dev --locked
cd your-project
glass project inspect
glass
```

Press `F7` for Development mode. The file tree is on the left, the native
editor and live application share the center, and processes, diagnostics,
actors, and the attributed timeline remain visible on the right. `F1` through
`F6` retain the browser workspace views.

On a narrow SSH or Mosh terminal, Glass starts directly in the single-pane
Development workspace. Use `1` through `5` or `Tab` instead of function keys.
The phone workspace disables continuous pixel streaming and keeps structured
browser semantics as the App view. See
[Mobile and remote development](mobile-remote.md) for Herdr persistence and
the private Safari tunnel workflow.

## First live loop

```console
glass project inspect
glass project files
glass project run dev --command "npm run dev" --wait
glass project diagnostics src/main.rs
glass project graph discover
glass project diff
```

Persistent interactive dev servers belong in the TUI. A one-shot CLI or MCP
invocation must use `--wait`/`wait: true`; Glass rejects a detached request
because the calling process would no longer own its PTY.

Inside the TUI command bar:

```text
project open src/main.rs
project search checkout
project run dev npm run dev
project diagnostics src/main.rs
project graph
project pi models
project pi prompt Explain @diagnostic and @entity:action.checkout.submit
```

Editor keys are deliberately small and predictable: arrow keys move, typing
edits, `Enter` and `Tab` insert text, `Backspace`/`Delete` remove text,
`Ctrl-S` performs an atomic attributed save, `Ctrl-Z`/`Ctrl-Y` undo and redo,
and `Esc` returns focus to the command bar. Mouse selection can place the
cursor; clicks in the live pane remain browser input.

## Evidence rules

- File mutations are confined to the canonical project root and fail on an
  externally changed buffer.
- Process logs and timeline history are bounded.
- A save records live-update state as pending. It becomes confirmed only when
  an attached browser reports a strictly newer semantic revision.
- Source/runtime links always include provenance and confidence. Explicit
  `data-glass-entity="…"` markers provide the strongest built-in bridge.
- Structured semantic observation is the browser context default. A visual
  diff says `not-captured` until a caller explicitly asks for a screenshot.
- Local harness events store prompt byte counts and hashes, not prompt text.

## Agents and Neovim

`glass agent ... --harness pi` uses the LF-delimited Pi RPC adapter owned by
the Glass harness contract; the TUI keeps that adapter alive across requests.
The adapter supports prompt, model list,
model selection, thinking level, follow-up, steer, abort, and new-session
requests. Pi is optional; normal project and browser use does not require it.

External agents can independently use `glass project ...`, MCP `project.*`
tools, or `glass project attach AGENT`. Their actor and authority are recorded
in the same timeline. Conflicting edit claims fail closed.

`glass project neovim probe` checks both a normal Neovim executable and the
headless RPC prototype. `glass project neovim start` runs compatibility Mode A
in a managed PTY. The v0.3.2 architecture decision is to retain Glass-native
rendering while using Neovim RPC as an optional editing engine, not as the
owner of browser, process, collaboration, or development-graph state.

See the [surface atlas](design/v0.3.2/development-surface-atlas.svg) for the
coherent editor, live app, palette, process, review, diff, replay, graph,
workflow, experiment, and collaboration layouts.
