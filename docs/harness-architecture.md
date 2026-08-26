# Glass Coding Harness Architecture

Glass follows the production harness patterns validated against Mastra Code,
Pleiades, and zerostack: the agent conversation is a durable channel, raw
runtime events are reduced into display state, and tools never need to occupy
the terminal rendering task.

The current Pi reference implementation was reviewed from its public SDK,
extension, TUI, keybinding, and session-format documentation, plus the Pi
architecture essay and community `pi-open-tui` package. The useful lessons are
specific: lifecycle events are the source of truth; `steer` and `followUp` are
separate queues; sessions are durable trees; custom UI components must be
width-bounded and explicitly invalidated; and extensions expose status,
widgets, renderers, and abort signals instead of printing implementation data.

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
- `:actions` opens guided actions for the current surface.
- `:` remains the expert command palette, not the only way to discover work.
- `Ctrl-C` is a global quit reflex in every input mode.
- `Ctrl-X` aborts the selected agent; `Ctrl-D` toggles steer/follow-up mode in the composer.
- `[/]` switches editor buffers; `:git diff` opens an inline Git diff;
  `:browser view` toggles embedded live browser pixels.
- `review` prepares a bounded evidence-aware review prompt; `harness list` and
  `harness start NAME` expose safe handoff to installed external harnesses.
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
Its semantic selection movement is local-only; it does not issue a CDP highlight
request for every arrow key. Standalone browser controls and visual screenshots
remain asynchronous operations in that separate TUI, while the `glass-dev`
workspace uses `SnapshotWorker` for governed background jobs.
