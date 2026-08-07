# Glass v0.3.2 release-candidate delivery analysis

Status: Active local development

Issue #32 describes Glass as a terminal-native live software development
environment. It is an epic, so the candidate needs a small number of real
cross-module paths that can be demonstrated end to end:

```text
project root
    ↓
detection/config → files/editor → processes
    ↓                  ↓             ↓
events/timeline ← source/runtime graph → diff
    ↓
CLI / MCP / TUI / harness
    ↓
bounded evidence and release gates
```

## Scope and boundaries

The candidate implements:

- deterministic project discovery for Cargo, Node, Python, and Go roots;
- optional `glass.toml` command/browser/editor/agent configuration;
- bounded, workspace-confined file tree, file reads, native text buffers, and
  atomic saves with actor attribution;
- a cross-platform PTY process manager with bounded output tails and lifecycle
  events;
- a Glass-owned actor/event/timeline contract;
- explicit source/runtime links carrying evidence, provenance, and confidence;
- code, process, semantic, and workflow impact projections in `glass diff`;
- one structured project-runtime command family shared by CLI, MCP, and the
  native TUI command palette;
- a deterministic local harness adapter proving prompt, stream, tool-call,
  tool-result, steering, and error transitions without making a model or cloud
  service a runtime dependency;
- release metadata, documentation, package validation, and a local candidate
  checkpoint for `0.3.2`.

The candidate does not fabricate framework source maps, LSP results, HMR
success, or Neovim RPC support. Those surfaces expose explicit capability
status and provenance. PTY compatibility is real; a full Glass-rendered
Neovim engine remains a documented follow-up unless its proof is present.

## Module decomposition

| Module | Responsibility | Depends on | Verification |
|---|---|---|---|
| `development::project` | root detection, `glass.toml`, bounded file/editor API | filesystem, git metadata | fixture-backed unit tests |
| `development::process` | PTY start/stop, output tail, health | `portable-pty` | lifecycle and output tests |
| `development::events` | actors, typed events, bounded timeline | serde, local storage | serialization and append tests |
| `development::graph` | source/runtime links and confidence | project paths | explicit/ambiguous mapping tests |
| `development::diff` | code/runtime/semantic/workflow projection | git/events | deterministic projection tests |
| CLI/MCP adapters | one typed operation family across surfaces | development core | structured integration tests |
| TUI surface/harness | native project view and local agent protocol | core + existing browser TUI | parser, rendering-state, harness tests |

## Integration enumeration

1. `ProjectWorkspace` creates `ProjectDetector`, file/editor state,
   `ProcessManager`, `Timeline`, and graph state from one root.
2. Process lifecycle and file saves emit real development events into the
   timeline; no mock event bus is used.
3. CLI and MCP decode the same project operation types and call the same core
   methods.
4. The TUI renders the same project snapshot and sends file/process/agent
   operations through the core.
5. The local harness emits tool calls that execute against the same workspace,
   then records actor and result events.
6. `diff` consumes git/process/event/graph state and reports a bounded
   cross-surface projection.
7. Release checks validate both the browser package and the `glass-dev`
   umbrella package without publishing either one.

## Candidate acceptance scenario

From a real repository, a maintainer can run `glass project inspect --json`,
list and read files, edit and save a buffer, start a dev/test command in a PTY,
inspect bounded output, link a runtime entity to a source location with explicit
provenance, ask the local harness to read that file, and inspect a structured
diff/timeline through CLI, MCP, or the TUI without starting Chrome. When a
browser is available, the existing Browser Workspace remains available as the
live application pane and semantic observation stays the default observation
path.
