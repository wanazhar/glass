# Development Runtime

**Status: Current 0.3.14 source behavior.** This guide describes the
`glass-dev` development workspace, not the browser-only crate. It owns one
canonical project root, bounded files and editor buffers, PTY processes,
language services, graph and timeline evidence, resident agents, tasks, and
optional browser context. The TUI, MCP, daemon, and Rust clients use these
Rust-owned services; they do not maintain separate project state.

See [Architecture overview](architecture/README.md) for module ownership. See [CLI reference](cli.md) for installed syntax and [Native Pi SDK runtime](pi-sdk-runtime.md) for agent lifecycle.

## Boundary and startup

Install the complete product and inspect a project without starting Chrome:

```console
cargo install glass-dev --locked
cd /path/to/project
glass project inspect --root .
```

`ProjectWorkspace::open` canonicalizes the root, detects project markers, loads `glass.toml` (preferred) or `.glass.toml`, opens the bounded timeline, and initializes resident services. Detection does not execute a detected command. `glass update --dry-run` and `glass update` update the existing Cargo-installed owner; see [Installation and operations](installation.md#update-a-cargo-installation).

A new or changed filesystem identity opens as `Untrusted`. Inspection, static reads, search, Git metadata, and manual browser operations remain available. Project hooks, skills, configured tools, tests, language-server overrides, Pi, kernels, experiments, and task execution require a local trust decision. `--yolo` does not bypass workspace trust. See [Workspace trust](workspace-trust.md).

The one-shot CLI opens and closes a workspace for one request. A resident TUI owns one workspace until exit. A stdio MCP server keeps a bounded registry for its process. The daemon shares workspaces between authenticated local clients. Registries hold at most eight projects and evict an idle project after 30 minutes. Closing a workspace terminates owned services and processes; it never deletes project files.

## Ownership and lifecycle

```text
canonical project root
        │
        ▼
DevelopmentWorkspace (generation + trust)
 ├─ ProjectWorkspace (tree, buffers, editor revisions, timeline, graph)
 ├─ ProcessManager (owned PTY trees)
 ├─ LanguageService / TestService / Debugger sessions
 ├─ AgentRegistry ── one worker + one Pi session per resident agent
 ├─ TaskScheduler ── task records above AgentRegistry
 ├─ DevelopmentToolRouter (revision, trust, actor, confirmation)
 └─ BrowserService (optional attached browser context)
        │
        └─ TUI SnapshotWorker (display projections only; never state owner)
```

`DevelopmentWorkspace` owns the generation and resident service registries. `ProjectWorkspace` owns project files, in-memory buffers, editor revisions, actor attribution, comments, proposals, checkpoints, timeline, and graph. `AgentRegistry` owns worker threads and Pi child processes. `TaskScheduler` owns task records but is currently in-memory; the daemon ticks it while its workspace actor is alive. Pi session files and timeline/checkpoint files are separate persisted artifacts. A worker, TUI snapshot, or external harness cannot become the project authority.

Every governed tool carries expected workspace generation and project revision. A stale call fails with `Conflict`; reread state and issue a new call. Mutations also require a trusted workspace, actor attribution, and the applicable mutation authority or confirmation.

## Project files and tree

Use bounded project operations:

```console
glass project files --root .
glass project read README.md --root .
glass project search BrowserSession --root .
glass project edit notes.txt --content "reviewed\n" --root .
glass project mkdir docs/drafts --root .
glass project rename notes.txt docs/drafts/notes.txt --root .
glass project delete docs/drafts/notes.txt --yes --root .
```

| Resource | Current bound and state |
|---|---|
| One file read or write | 512 KiB; valid UTF-8 for reads |
| Editor or existing-file buffer | 1 MiB |
| Tree inventory | 2,048 entries; result reports `truncated`, ignored directories, and skipped symlinks |
| Relative path | 512 bytes; parent traversal, absolute paths, and symlink escape fail |
| Timeline | 512 retained events with bounded payloads |
| Workspace actors | 64 actor identities |

The tree skips generated/vendor roots such as `.git`, `target`, `node_modules`, and `.glass` before consuming the inventory bound. It does not follow unknown or external symlinks. Delete removes one file or empty directory only; it does not recursively remove a tree. Treat `truncated: true` as incomplete inventory.

## Native editor and conflict rules

Opening a file creates an in-memory `EditorBuffer` with content, disk `originalHash`, dirty flag, one-based cursor, optional selection, and actor. Buffers are not automatically persisted. `Ctrl-S` or `glass.editor.save` compares the current disk hash with `originalHash` and then writes atomically through the project actor. Undo and redo retain at most 256 content entries per path. A save never overwrites an external change.

The native editor supports arrows, text insertion, `Enter`, `Backspace`, `Shift` plus arrows for a selection, `Ctrl-S`, `Ctrl-Z`, `Ctrl-Y`, and `Alt-A` to prepare an agent prompt. The editor starts in INSERT. `Esc` returns to NORMAL. `Esc` from NORMAL on a clean buffer leaves the editor. Unsaved content still opens the exit prompt (`S` save, `D` discard, `Q` discard and quit, `Esc`/`N` stay). `Ctrl-C` opens Glass quit confirmation from editor input; if the unsaved-exit prompt is already open, its save/discard/stay choices take priority. `Tab` remains product navigation outside editor mode. `Delete` has no editor mutation binding.

The editor's native layer also supports modal motions, operators and tree-sitter
textobjects with lexical fallback, local or resident-Pi FIM ghosts, LSP hover,
definition, references, symbols and inlay hints, Git/Agent/Page/Proof/comment
gutter marks, and bounded proposal hunk review. Agent pair-apply streams a
proposal until the human accepts, rejects, or yields it; prove-it predicates
only become evidence after a live browser verification passes.

`Alt-W` toggles soft wrapping. With wrapping enabled, source lines reflow to the available editor width, the horizontal scroll resets, and cursor visibility is calculated from the visual wrapped row. With wrapping disabled, source lines remain one row each and horizontal scrolling follows source columns. The renderer measures terminal cell width, preserves syntax and selection backgrounds, pads continuation rows under the line-number gutter, and renders a cursor cell even at end-of-line. This is presentation state only; the buffer cursor and selection remain one-based source positions.

Editor collaboration is explicit, not automatic. `ProjectWorkspace::collaboration().claim(EditClaim)` records a read or write claim and rejects overlapping write claims from different actors. Opening or saving a buffer does **not** create or enforce a claim. Claim subscribers receive a bounded event stream; `release_actor` removes that actor's claims. Comments, proposals, and checkpoints are separate state:

| State | API/tool examples | Recovery |
|---|---|---|
| Comment `open`/`resolved` | `glass.editor.comment.add`, `.resolve` | Resolve an existing comment; invalid ranges are rejected |
| Proposal `pending`/`accepted`/`rejected`/`stale` | `.proposal.create`, `.accept`, `.reject` | Recreate from current content when base content changed |
| Checkpoint | `.checkpoint.create`, `.restore` | Restore open buffers; inspect disk before saving |

A proposal must match the exact original content and hash. A changed buffer marks it stale and does not overwrite the buffer. Exact selection replacement is Unicode-safe and clears the selection. Neovim is optional and remains a managed editing engine; Glass retains project, actor, browser, process, graph, and timeline ownership. See [Development TUI architecture](architecture/development-tui.md).

## Processes and PTYs

Start finite or resident work:

```console
glass project run check --command "cargo check" --wait --root .
```

In the TUI command palette, use `process start NAME COMMAND`; through the router use `glass.process.start` with `wait: false` for a resident process. Every managed process uses a real PTY, a 32 KiB retained output tail, a unique name, and explicit `running`, `exited`, `stopped`, or `failed` state plus `starting`, `healthy`, `exited`, `stopped`, or `failed` health. A project owns at most 32 registered processes. Input is capped at 16 KiB. PTY resize dimensions must be non-zero.

Glass owns the process tree. Unix uses a process group; Windows uses a kill-on-close Job Object. `stop` performs graceful termination and bounded escalation. `restart` stops and removes the old record before starting the saved command. `remove` requires a stopped process. Poll failures are surfaced as failed state and retained output, not discarded. An exited record remains until removal or workspace close.

## Diagnostics and persistent LSP

Detected commands supply defaults only when project evidence exists:

```console
glass project test --root .
glass project lint --root .
glass project diagnostics src/main.rs --root .
```

Use `project run` for another command. Exit status is authoritative; empty diagnostics do not turn a failed command into success. PTY output, test runs, LSP events, graph links, and live-update proofs are attributed to actors and revisions.

The resident language service owns one server and JSON-RPC channel. It sends `initialize`, monotonic `didOpen`/`didChange` versions, `didSave`, `didClose`, then `shutdown`/`exit`. Missing executables, malformed responses, protocol errors, and timeouts are explicit failures. Detected names include `rust-analyzer`, `typescript-language-server`, `pyright-langserver`, and `gopls`; installation is external to Glass.

## Source/runtime graph and live evidence

A source save is only a pending live update. It becomes confirmed after an attached browser reports a strictly newer compatible semantic revision. Without that evidence, Glass reports pending rather than claiming hot reload.

## Timeline, inbox, and replay

`project timeline` and `project replay` inspect bounded attributed events; replay reconstructs state and never re-executes shell commands or browser input.

## Agents and actor authority

Glass has four distinct agent paths:

| Path | Ownership and behavior | Entry point |
|---|---|---|
| Deterministic local harness | Synchronous, browser-independent `glass.harness.v1` reducer. It resolves bounded references and scripted prompts such as `read PATH`, `files`, `process list`, and `diff`; it has no model provider or persistent session. | `glass agent hello/prompt --harness local --root .` |
| Native Pi resident agent | `AgentRegistry` worker with a persistent native `AgentSession`, governed Glass tools, and optional browser context. | TUI Agent surface; `glass.agent.*`; see [Native Pi SDK runtime](pi-sdk-runtime.md) |
| External harness handoff | Fixed executable names discovered on `PATH`; Glass hands the terminal to the selected interactive program and resumes after exit. It does not emulate or register that program. | `glass harness list`; `glass harness start NAME --root .` |
| One-shot delegation | Temporary Codex, Claude, or OpenCode child. Read-only is default; output and errors are bounded; no resident identity or session is created. | `glass agent delegate HARNESS PROMPT --root .` |

`glass agent prompt --harness pi` is the compatibility Pi command adapter for a one-shot CLI request. It is not the resident native `AgentSession` path. Use the TUI or daemon-backed resident agent for steering, follow-up, approvals, and persistent ownership.

The trusted native Pi tool gateway provides read, list, search, edit, write, process, test, LSP, Git, browser, workflow, debugger, and task capabilities according to availability. A mutation is checked again at execution time; stale revisions, leases, path confinement, trust, and confirmation still apply. See [MCP integration](mcp.md) for transport semantics.

## Failure and recovery matrix

| Failure | Effect | Recovery |
|---|---|---|
| Path outside canonical root, invalid UTF-8, or size bound | No mutation; `PathOutsideWorkspace` or `InvalidInput` | Use a relative in-root path and a bounded UTF-8 file |
| External disk change or stale proposal | Save/accept fails with `Conflict`; disk is unchanged | Reread, compare, reopen or recreate proposal, then retry |
| Explicit write-claim overlap | New claim fails; existing buffers remain | Release/reconcile the actor claim and claim a non-overlapping range |
| PTY poll, process, or LSP failure | State is failed/degraded; retained output/evidence remains | Inspect, stop/restart the owned service, or install the missing executable |
| Agent worker/Pi process exits | Agent becomes `failed`; dependent agents/tasks do not become success | Inspect `lastError`, restart a failed/cancelled agent or create a new session |
| Browser disconnect or recovery | Browser tools and revisions become unavailable; project remains alive | Reconnect/attach or continue in semantic-only mode |
| Workspace generation/revision conflict | Tool is rejected before execution | Inspect current workspace and issue a fresh call |
| TUI exit with unsaved buffer | Exit prompt blocks discard | Save, or explicitly choose the discard path |

## Verification and evidence

Source-level evidence includes:

- `development::project::tests::file_writes_are_confined_and_atomic` and `project_tree_reports_ignore_and_symlink_semantics`;
- `editor_save_records_actor_and_clears_dirty_state`, `editor_undo_redo_and_external_change_conflicts_are_explicit`, `editor_selection_replacement_is_atomic_and_unicode_safe`, and `editor_comments_replay_and_proposals_require_approval`;
- `editor_checkpoints_restore_unsaved_buffers_and_survive_reopen` and `editor_proposals_become_stale_on_buffer_conflict`;
- `development::collaboration::tests` for explicit overlapping-write claims;
- `process` tests for PTY lifecycle, output bounds, stop/restart, and degraded polling;
- `tui::state` tests for soft-wrap cursor scrolling, Shift selection, and modal exit behavior; and
- `tui::snapshot::tests::snapshot_worker_publishes_and_applies_without_blocking_ui` for non-blocking projection.

Run the targeted tests from the repository when validating a change. Do not treat the presence of a resident agent, a detected command, or a browser connection as proof that its work succeeded; inspect the returned status and evidence.

## Related guides

- [Native Pi SDK runtime](pi-sdk-runtime.md)
- [Autonomous task DAGs](task-dag.md)
- [Coding harness architecture](harness-architecture.md)
- [Development TUI architecture](architecture/development-tui.md)
- [Local daemon](daemon.md)
- [Workspace trust](workspace-trust.md)
- [CLI reference](cli.md)
- [Security](../SECURITY.md)
