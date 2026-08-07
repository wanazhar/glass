# Glass v0.3.2 issue #32 delivery analysis

Status: Implemented locally; publication remains a maintainer action

Issue #32 and both issue comments are the authoritative contract. The earlier
three-task “thin vertical slice” interpretation was insufficient: the release
gate describes an integrated development environment and the packaging comment
requires two real public products. This matrix is the review boundary.

## Architecture

```text
glass-dev (`glass`)
  ├─ native Development TUI
  ├─ project/editor/LSP/PTY/agent/graph runtime
  └─ exact =0.3.2 dependency ───────────┐
                                        ↓
glass-browser (`glass-browser`, `glass_browser`)
  └─ browser sessions, structured observation, workflows, MCP, terminal browser
```

The workspace root is virtual (`resolver = "3"`), publication is browser
first, and the workflow waits for crates.io visibility before packaging,
publishing, and clean-installing `glass-dev`. Executable names do not collide.
The browser product hides and rejects the project/agent commands and disables
Development mode; it never installs Pi or Node. The dependency direction is
one-way.

## Pillar evidence matrix

| # | Issue pillar | Implemented evidence |
|---:|---|---|
| I | Project runtime | Cargo/Node/Python/Go detection, `glass.toml`, commands, Git branch, formatter/LSP inventory, local URL config. |
| II | File explorer | Bounded Git-aware tree, read/create/edit/mkdir/rename/move/safe-delete/search, dirty and actor markers. |
| III | Native editor | Multiple buffers, line numbers, cursor/mouse editing, atomic save/conflict detection, undo/redo, search/replace, syntax/bracket/fold primitives. |
| IV | Neovim | Real PTY launch plus headless Lua/RPC probe and explicit optional-engine decision. |
| V | Language services | Bounded Content-Length LSP client; real rust-analyzer initialize/didOpen/publishDiagnostics test. |
| VI | PTY/processes | Interactive PTY input/resize, start/stop/restart/remove, bounded logs, exit/health/cwd/URL state and events. |
| VII | Harness | Glass-owned requests/events/tools; local adapter and real LF-JSON Pi RPC adapter for prompt, steer, follow-up, abort, models, thinking, and session reset. |
| VIII | Semantic context | Bounded resolver for workspace, file hash, diagnostics, run, page/browser status, entity, and revision references. |
| IX | Tool registry | Typed file/process/Git/semantic/test/runtime tools; unattached browser/workflow/memory calls fail explicitly. |
| X | Actors/collaboration | Typed identity/session/capability/authority/connection, bounded bus, ownership claims, overlap conflict, join/leave/action events. |
| XI | External agents | CLI and strict JSONL MCP project tools plus explicit actor attachment; no embedded-agent dependency. |
| XII | Development graph | Persistent bidirectional links with validated locations, provenance, confidence, and explicit marker discovery. |
| XIII | Source/runtime navigation | Entity→source and source→entity queries; TUI live pane and graph commands retain uncertainty. |
| XIV | Live changes | Saves emit pending state; only a newer browser semantic revision can confirm a live update. A real-browser smoke proves edit→served update→structured observation. |
| XV | Semantic breakpoints | Disappearance, name loss, role change, and actionability loss with likely-source evidence. |
| XVI | Workflow as test | Existing semantic workflow runtime remains shared across CLI/MCP/Rust; project verification emits test lifecycle evidence. |
| XVII | Unified diff | Code numstat, PTY/runtime, semantic breakpoint, workflow/test, and explicit `visual: not-captured` projections. |
| XVIII | Agent edits | Actor-attributed patch tool, Git diff, before/after hashes, claims, timeline, and replay. |
| XIX | Collaborative buffers | Explicit file/range read-write claims; overlapping writers fail closed without pretending CRDT support. |
| XX | Timeline/replay | Bounded, atomically persisted newest events and attributed revision windows. |
| XXI | Experiments | Real Git worktrees, branches, separate ports/browser URLs/agent thread IDs, and measured numstat comparisons. |
| XXII | Agent controls | Pi model/thinking/session/follow-up/abort controls; the TUI owns a long-lived non-blocking adapter. |
| XXIII | Optional subagents | No rigid swarm is required or enabled by default; external actors and isolated experiment workers provide the bounded extension point. |
| XXIV | Search/palette | Ranked search across files, entities, processes, diagnostics/events, and commands in CLI/MCP/TUI. |

## Integrated proof paths

1. `development_edit_reaches_live_browser_and_semantic_revision` starts a real
   managed local server and Chrome, edits an explicitly linked source file,
   observes the updated accessible text through structured observation, proves
   a newer browser revision, and records confirmed runtime evidence.
2. `lsp_reports_real_rust_diagnostic_when_rust_analyzer_is_available` exercises
   a real rust-analyzer subprocess and protocol frames.
3. `pi_adapter_negotiates_real_rpc_state_when_pi_is_available` exercises the
   installed Pi RPC process without making it a required dependency.
4. Neovim tests prove the installed executable and headless prototype; PTY
   process tests cover output, URL detection, input, resize, restart, and exit.
5. MCP integration retains strict one-JSON-value-per-line stdout while
   external project operations use the same bounded core.

## TUI concurrency and visual contract

The implemented layout is files/Git (21%), editor plus live app/semantics
(54%), runtime/tests/actors (25%), with agent/timeline below. Browser graphics
use the exact live-pane geometry. LSP and Pi requests use worker channels, so
input and rendering continue. The complete comment-requested design inventory
is in `docs/design/v0.3.2/development-surface-atlas.svg`; the three baseline
mockups remain alongside it.

## Release gates

The local gate is formatting, locked workspace all-target tests, warnings-as-
errors Clippy, warning-free rustdoc, the complete real-browser smoke suite,
version/docs/corpus validators, browser package verification, patched-source
pre-publication validation of the exact dev dependency, and both registry
install smokes after ordered publication. No local result is represented as a
tag, crates.io publication, GitHub Release, or certification for an untested
OS.
