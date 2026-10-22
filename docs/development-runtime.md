# Development Runtime

**Status: Current source behavior.** This guide describes the `glass-dev`
workspace. It does not describe the browser-only product. The workspace owns
one project root and the project's files, editor buffers, terminal processes,
language services, event history, agents, tasks, and optional browser session.
The TUI, MCP server, daemon, and Rust API use the same services. They do not
keep separate copies of project state.

See [Architecture overview](architecture/README.md) for module ownership. See [CLI reference](cli.md) for installed syntax and [Native Pi SDK runtime](pi-sdk-runtime.md) for agent lifecycle.

## Boundary and startup

Install the complete product and inspect a project without starting Chrome:

```console
cargo install glass-dev --locked
cd /path/to/project
glass project inspect --root .
```

`ProjectWorkspace::open` resolves the canonical project root, detects project
files, loads `glass.toml` or `.glass.toml`, opens the bounded event history,
and starts resident services. Detection does not run commands from the
project. Use `glass update --dry-run` to inspect an update before applying it.
See [Installation and operations](installation.md#update-a-cargo-installation)
for package ownership and recovery details.

A project with a new or changed filesystem identity opens as `Untrusted`.
Inspection, static reads, search, Git metadata, and manual browser actions are
available. Project hooks, configured tools and tests, language-server
overrides, kernels, experiments, and task execution need a local trust
decision. The default Glass Core agent uses the same workspace tool router.
The legacy Pi backend follows the same workspace authority rules. Glass does
not trust a project because its configuration requests trust.

`--yolo` is a process-local Glass Dev option. It enables repository
configuration and bypasses the workspace-trust execution gate for that
process. It does not save a trust decision. Root confinement, revisions,
leases, protocol limits, timeouts, and result bounds still apply. See
[Workspace trust](workspace-trust.md).

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
 ├─ AgentRegistry ── one worker per resident agent
 │                   └─ Glass Core by default; Pi is an explicit legacy backend
 ├─ TaskScheduler ── task records above AgentRegistry
 ├─ DevelopmentToolRouter (revision, trust, actor, confirmation)
 └─ BrowserService (optional attached browser context)
        │
        └─ TUI SnapshotWorker (display projections only; never state owner)
```

`DevelopmentWorkspace` owns the workspace generation and resident-service
registries. `ProjectWorkspace` owns files, in-memory buffers, editor revisions,
actor attribution, comments, proposals, checkpoints, event history, and graph
data. `AgentRegistry` owns resident agent workers and the selected engine
runtime. Glass Core is the default. Set `GLASS_AGENT_HARNESS=pi` to use the
legacy Pi runtime. The task scheduler keeps task records in memory while the
daemon workspace actor is alive. Agent session files, timeline events, and
editor checkpoints have separate storage. A worker, TUI snapshot, or external
harness does not own the project.

`project inspect` reports detected LSP executable names and configured browser
URL hints as discovery metadata. Those values are informational: Glass does not
launch detected language servers or infer extra development URLs unless the
project configuration explicitly requests them.

The Glass TUI's default agent controls use Core-owned provider and model
configuration. `:agent setup login` hands the terminal to the resolved
`glass-core` interactive UI in the current project; Core's `/login` owns
authentication, and Glass refreshes readiness after return without restarting
resident sessions. To start an agent with configuration changed during that
handoff, use `:agent spawn ROLE TASK`, which creates a separate resident
runtime. `:agent new` creates a conversation in the existing runtime and does
not reload its configuration. `:agent models [AGENT_ID]` and
`:agent model [AGENT_ID] PROVIDER MODEL` use the selected session, or the
workspace's first active session when none is selected, through negotiated
model APIs. Model catalog responses are shown in the Agent conversation. Setting
`GLASS_AGENT_HARNESS=pi` retains the Pi-specific setup, login, and model path;
see the [resident harness guide](glass-core-harness.md).

Live resident agents have separate Agent TUI transcripts and composer drafts;
switching agents restores that thread's text and cursor. A send result stays
with the AgentId captured at submission, including a first-send agent created
by the result. AgentId and these drafts are process-local: they are not restored
after Glass restarts and are not durable project thread identities.

Glass also records random, project-scoped ThreadId and RunId values plus the
actual initialized Glass Core session ID in the
`.glass/threads/registry.json` ledger. On Unix the registry, lock, and parent
directories use private modes. On Windows files inherit workspace ACLs; this
checkpoint does not strengthen those ACLs, so reopened rows remain unavailable
there until protected ACL support is implemented. Reopened rows with initialized Core
sessions appear as `recovery pending` on supported hosts; runs that never
initialized, including work still queued at shutdown, appear as `unavailable`.
Windows rows remain unavailable under the current inherited-ACL policy. These
states reject sends.
Use `:agent resume [AGENT_ID]` on a recovery-pending row to reattach its exact
stored Core session. Glass verifies the returned session ID, mixed tool mode,
and canonical project directory, then restores bounded user/assistant history
from Core session entries before enabling sends. Missing or mismatched sessions
remain recovery-pending with an error; Glass does not create a replacement or
replay a prompt. Only ThreadId, RunId, and the opaque Core session reference are
stored by the Glass ledger; provider configuration and credentials remain Core
owned. Composer drafts and follow-up queues are not persisted, and reopened
rows are not live workers until an explicit resume succeeds. See the
[exact-session recovery contract](plan/analysis/issue63-core-session-resume.md).
The TUI uses its ordinary confirmation for the resume action; subsequent
conversation follows the existing session flow without per-tool-call
confirmation.

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

Opening a file creates an in-memory `EditorBuffer` with content, disk `originalHash`, dirty flag, one-based cursor, optional selection, and actor. Buffers are not automatically persisted. While the TUI is open, it checks open buffer paths every 500 ms. A clean buffer adopts changed file contents; a dirty buffer keeps its text and exposes an external snapshot or deletion with unknown actor attribution. `:editor compare`, `:editor reload`, and `:editor keep-local` explicitly inspect or resolve that state. Reload clears undo/redo; keep-local rebases the buffer to the observed file identity and leaves it dirty. `Ctrl-S` or `glass.editor.save` compares the target file's existence and content hash at the atomic replacement boundary. It rejects a target whose identity differs at that check, while unrelated project revision changes do not block the save. This is an optimistic check, not a cross-process lock or compare-and-swap against writers that change the file after the check. Undo and redo retain at most 256 content entries per path.

The native editor supports arrows, text insertion, `Enter`, `Backspace`, `Shift` plus arrows for a selection, `Ctrl-S`, `Ctrl-Z`, `Ctrl-Y`, and `Alt-A` to prepare an agent prompt. The editor starts in INSERT. `Esc` returns to NORMAL. `Esc` from NORMAL on a clean buffer leaves the editor. Unsaved content still opens the exit prompt (`S` save, `D` discard, `Q` discard and quit, `Esc`/`N` stay). `Ctrl-C` opens Glass quit confirmation from editor input; if the unsaved-exit prompt is already open, its save/discard/stay choices take priority. `Tab` remains product navigation outside editor mode. `Delete` has no editor mutation binding.

The editor's native layer also supports modal motions, operators and tree-sitter
textobjects with lexical fallback, local or resident-agent FIM ghosts, LSP hover,
definition, references, symbols and inlay hints, Git/Agent/Page/Proof/comment
gutter marks, and bounded proposal hunk review. Agent pair-apply streams a
proposal until the human accepts, rejects, or yields it; prove-it predicates
only become evidence after a live browser verification passes.

`Alt-W` toggles soft wrapping. With wrapping enabled, source lines reflow to the available editor width, the horizontal scroll resets, and cursor visibility is calculated from the visual wrapped row. With wrapping disabled, source lines remain one row each and horizontal scrolling follows source columns. The renderer measures terminal cell width, preserves syntax and selection backgrounds, pads continuation rows under the line-number gutter, and renders a cursor cell even at end-of-line. This is presentation state only; the buffer cursor and selection remain one-based source positions.

Editor collaboration is explicit, not automatic. `ProjectWorkspace::collaboration().claim(EditClaim)` records a read or write claim and rejects partial overlapping write claims, including overlaps from the same actor. Repeating a claim for the same exact range replaces that claim. Opening or saving a buffer does **not** create or enforce a claim. Claim subscribers receive a bounded event stream. `release_actor` removes that actor's claims and publishes one `editor.released` event for each removed claim. Comments, proposals, and checkpoints are separate state:

| State | API/tool examples | Recovery |
|---|---|---|
| Comment `open`/`resolved` | `glass.editor.comment.add`, `.resolve` | Resolve an existing comment; invalid ranges are rejected |
| Proposal `pending`/`accepted`/`rejected`/`stale` | `.proposal.create`, `.accept`, `.reject` | Recreate from current content when base content changed |
| Checkpoint | `.checkpoint.create`, `.restore` | Restore open buffers; inspect disk before saving |

A proposal must match the exact original content and hash. A changed buffer marks it stale and does not overwrite the buffer. Exact selection replacement is Unicode-safe. It advances the project revision once, stores one undo state, clears redo and the selection, and publishes an editor event. Checkpoint restore validates all buffers before changing the project. It advances the revision once and records one undo state for each restored buffer that was already open. It recomputes each dirty flag from the buffer content and recorded original hash. It moves invalid cursors to the end of the restored content and clears invalid selections. Restore changes in-memory buffers only; save them explicitly to write to disk. Neovim is optional and remains a managed editing engine; Glass retains project, actor, browser, process, graph, and timeline ownership. See [Development TUI architecture](architecture/development-tui.md).

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
| Resident agent (default) | `AgentRegistry` worker with a persistent Glass Core session, Glass-owned tools, and optional browser context. | TUI Agent surface; `glass.agent.*`; see [Glass Core resident harness](glass-core-harness.md) |
| Resident agent (legacy Pi) | The same registry worker uses the Pi `AgentSession` when `GLASS_AGENT_HARNESS=pi`. | TUI Agent surface; [Native Pi SDK runtime](pi-sdk-runtime.md) |
| External harness handoff | Fixed executable names discovered on `PATH`; Glass hands the terminal to the selected interactive program and resumes after exit. It does not emulate or register that program. | `glass harness list`; `glass harness start NAME --root .` |
| One-shot delegation | Temporary Codex, Claude, or OpenCode child. Read-only is default; output and errors are bounded; no resident identity or session is created. | `glass agent delegate HARNESS PROMPT --root .` |

`glass agent prompt --harness pi` is a one-shot compatibility command. It is
not the resident agent. Use the TUI or daemon-backed agent for steering,
follow-up messages, and a persistent session.

The trusted resident tool gateway provides read, list, search, edit, write, process, test, LSP, Git, browser, workflow, debugger, and task capabilities according to availability. The resident engine receives this catalog once, when its session opens, and Glass executes every call; a mutation is checked again at execution time, where stale revisions, leases, path confinement, and confirmation still apply. See [MCP integration](mcp.md) for transport semantics.

## Failure and recovery matrix

| Failure | Effect | Recovery |
|---|---|---|
| Path outside canonical root, invalid UTF-8, or size bound | No mutation; `PathOutsideWorkspace` or `InvalidInput` | Use a relative in-root path and a bounded UTF-8 file |
| External disk change or stale proposal | Save/accept fails with `Conflict`; disk is unchanged | Reread, compare, reopen or recreate proposal, then retry |
| Explicit write-claim overlap | New claim fails; existing buffers remain | Release/reconcile the actor claim and claim a non-overlapping range |
| PTY poll, process, or LSP failure | State is failed/degraded; retained output/evidence remains | Inspect, stop/restart the owned service, or install the missing executable |
| Resident agent worker or engine exits | Agent becomes `failed`; dependent tasks do not become successful | Inspect `lastError`, restart the agent or create a new session |
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

- [Glass Core resident harness](glass-core-harness.md)
- [Native Pi SDK runtime](pi-sdk-runtime.md)
- [Autonomous task DAGs](task-dag.md)
- [Coding harness architecture](harness-architecture.md)
- [Development TUI architecture](architecture/development-tui.md)
- [Local daemon](daemon.md)
- [Workspace trust](workspace-trust.md)
- [CLI reference](cli.md)
- [Security](../SECURITY.md)
