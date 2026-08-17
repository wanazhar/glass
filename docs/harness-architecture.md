# Glass Coding Harness Architecture

Glass follows the production harness patterns validated against Mastra Code,
Pleiades, and zerostack: the agent conversation is a durable channel, raw
runtime events are reduced into display state, and tools never need to occupy
the terminal rendering task.

## Runtime planes

```text
terminal input ───────┐
resize / mouse ───────┼──> TUI reducer ──> immediate render
agent event channel ──┤          │
browser/tool jobs ────┘          ▼
                         bounded actor requests
                                  │
              ┌───────────────────┴───────────────────┐
              │ Glass workspace actor                 │
              │ shared workspace + governed tools     │
              │ file/git/agent/test/browser refresh   │
              └───────────────────┬───────────────────┘
                                  ▼
                         versioned DisplaySnapshot
                                  │
                         latest-frame mailbox
                                  ▼
                              TUI panes
```

The render loop never performs the expensive refresh pass. It applies the
latest versioned snapshot and keeps accepting input while Git, agent history,
CDP screenshots, or governed mutations are in flight. A refresh latency over
200ms is surfaced in the status line rather than hidden.

## Event and snapshot rules

- Agent conversation updates use `history(since)` and a monotonic event cursor;
  the UI does not rebuild the full transcript on every frame.
- Snapshot replacement uses an explicit monotonic version, not content or timing
  heuristics.
- Every snapshot contains all resident projections: agent, task, editor, LSP,
  process, Git, test, kernel, debugger, replay, workflow, browser, trust, and
  onboarding state.
- Tool mutations require the existing typed authority/revision/confirmation
  contract. Confirmed tools are queued on the actor and return a typed result;
  active jobs do not delay terminal restoration on exit.
- Refresh and conversation requests are coalesced atomically, so a blocked
  operation cannot create an unbounded backlog.
- Browser screenshots use a worker visual job and latest-frame delivery; stale
  frames never queue without bound.

## Human interaction model

The primary route is visible and keyboard-first:

- `?` opens the unified keyboard cockpit.
- `a` opens actions for the current surface.
- `:` remains the expert command palette, not the only way to discover work.
- `Ctrl-C` is a global quit reflex in every input mode.
- `Ctrl-X` aborts the selected agent; `Ctrl-S` steers from the composer.
- `[/]` switches editor buffers; `d` opens an inline Git diff; `v` toggles
  embedded live browser pixels.
- Paging keys and mouse wheel scroll content; clicks select navigation or
  semantic browser entities.

## Verification

The architecture is checked at three levels:

1. projection and reducer unit tests;
2. snapshot-worker integration tests for latest-frame and complete-surface
   coverage;
3. opt-in PTY tests (`GLASS_E2E=1`) driving the release binary at desktop and
   phone sizes, including Ctrl-C, action menus, palette navigation, and surface
   switching.

The standalone Browser TUI shares the same semantic revision/recovery contract
and uses Herdr when available, otherwise bounded ANSI half-block rendering.
