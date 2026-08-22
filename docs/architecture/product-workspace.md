# Glass Product Workspace

Status: Current 0.3.12 source information architecture

## Product information architecture

Glass Dev presents work, not its internal service registry. Desktop has seven
primary destinations and phone has five. LSP, kernels, replay, experiments,
daemon details, and multi-agent orchestration remain reachable through context
panes, `More`, and the command palette.

```text
Desktop: Agent · Code · App · Terminal · Tasks · Git · Debug
Phone:   Agent · Code · App · Tasks · More
```

The same `DevelopmentWorkspace` remains authoritative. Navigation changes only
the projection and focus; it does not create parallel state owners.

## Desktop layout

```text
┌ GLASS ─ project ─ branch ─ mode ─ trust/authority ────────────────────────┐
│ Files / Tasks       Main workspace                         Context         │
│ tree + changes      Agent / Code / App / Terminal / ...    selected item   │
│                     interactive primary surface            evidence/actions│
├───────────────────────────────────────────────────────────────────────────┤
│ Agent   Code   App   Terminal   Tasks   Git   Debug             : Commands│
└───────────────────────────────────────────────────────────────────────────┘
```

The context pane follows the selected code symbol, agent/tool event, debugger
frame, task evidence, Git file, process, or BrowserWorkspace entity. It does
not show a fixed dump of unrelated subsystems.

## Phone layout

```text
┌ GLASS ─ project ─ branch ─────────────┐
│ Agent · provider/model/session         │
├────────────────────────────────────────┤
│ conversation / selected focused view   │
├────────────────────────────────────────┤
│ > message or command                   │
├────────────────────────────────────────┤
│ Agent    Code    App    Tasks    More  │
└────────────────────────────────────────┘
```

Phone defaults to Agent after trust/onboarding. Each focused view owns its own
scroll position. `j`/`k` scroll content; `Tab` changes focus; direct navigation
keys and the bottom bar change destination. Advanced surfaces live in `More`.

## Agent conversation

Ordinary use has one current conversation and never requires an agent ID.
The composer supports cursor movement, bounded history, multiline display,
submit, abort, steering, follow-up, model/thinking selection, new/resume
session, and visible readiness. Tool calls render as concise attributed cards;
mutation requests render as a confirmation sheet. Multi-agent IDs appear only
in the orchestration view.

## Code, Git, and runtime projections

- Code owns a file tree, editable buffer viewport, cursor, selection, line
  numbers, dirty/conflict state, diagnostics, undo/redo, save, search, and file
  switching.
- Git owns branch/ahead-behind, changed-file list, selected diff, stage,
  unstage, discard confirmation, commit, and push confirmation.
- Terminal owns managed processes and interactive PTY input/output.
- Tasks distinguish `✓ verified`, `◇ settled/unverified`, `× failed`, and
  `! blocked/ambiguous` with evidence and operation progress.
- Debug owns stack, variables, watches, breakpoints, console, threads, and
  source navigation. LSP, tests, kernels, experiments, replay, and daemon jobs
  use compact designed cards in their contextual or More projections.

Raw JSON remains available only through explicit Inspect/Raw/JSON/Copy/Export
actions.

## Startup and discovery

Startup reads project identity, Git, workspace trust, and initial readiness
metadata synchronously, then renders the cockpit before the snapshot worker
hydrates the complete resident projections. Missing or degraded requirements
remain visible with a remediation action; slow files, agents, Git, browser, and
process projections do not delay the first frame.

## Palette and focus

The palette is generated from typed capabilities and contains fuzzy search,
descriptions, key hints, contextual ordering, recent commands, parameter hints,
history, completion, cursor editing, and visible validation errors. It is the
expert route, not the only normal interaction route.

Every focused pane has a visible focus marker and independent scroll. Mouse,
focus, resize, and bracketed paste events are processed only while their
terminal modes are enabled. Confirmation sheets and recovery overlays own
focus until accepted, denied, or dismissed.

## Tests

Scripted reducer tests at desktop, compact, 48x18, 64x24, and 80x24 exercise
trust, Agent conversation, App semantics, Code editing and scrolling, task
evidence, palette history/editing, More/runtime inspection, focus, mouse, and
resize. PTY tests verify alternate-screen cleanup and real key/paste paths.
