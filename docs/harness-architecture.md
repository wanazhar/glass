# Glass Coding Harness Architecture

**Status: Current 0.3.13 source behavior.** This guide defines worker boundaries and display-state flow in `glass-dev`. It does not make an external coding program part of Glass and does not change immutable release history. See [Development Runtime](development-runtime.md) for project semantics and [Native Pi SDK runtime](pi-sdk-runtime.md) for Pi protocol details.

## Runtime planes

Glass separates authoritative workspace state, agent execution, background projection, and terminal rendering:

```text
keyboard / resize / mouse
          │
          ▼
   TUI input reducer ───────────────┐
          │                         │
          │ immediate UI state      │ ActorRequest
          ▼                         ▼
   render latest frame       SnapshotWorker
                                  │
                     ┌────────────┴────────────┐
                     │                        │
              workspace lock            job workers
                     │                  tool / screenshot
                     ▼                        │
           immutable DisplaySnapshot ◄────────┘
                     │
                     ▼
                  TUI panes

resident agent worker ── Pi SDK IPC ──► DevelopmentToolRouter
external harness ─────── terminal handoff (not a Glass worker)
```

`DevelopmentWorkspace` owns files, buffers, processes, language servers, browser state, agents, tasks, and authority. The TUI owns selection, scroll, input mode, status, and modal state. `SnapshotWorker` reads and projects state; it is not a second workspace and cannot commit an optimistic mutation. The render loop never performs the full refresh pass and never waits for the workspace lock.

The four agent paths are intentionally different:

| Path | Lifetime and ownership | Capability boundary |
|---|---|---|
| Deterministic local harness | One synchronous `LocalHarness` request | No model provider, no Pi process, no persistent conversation; bounded references and scripted read/list/process/diff prompts |
| Native Pi resident agent | One `AgentRegistry` record, worker thread, `GlassPiRuntime`, and Pi `AgentSession` | Glass-registered tools through the resident router; persistent session and event evidence |
| External harness handoff | One installed interactive executable receives the terminal | Fixed catalog and PATH lookup; Glass does not emulate its protocol or own its session |
| One-shot delegation | One bounded Codex, Claude, or OpenCode child | Read-only default, bounded output, no resident identity or session |

## Event and snapshot rules

`SnapshotWorker::spawn` starts one `glass-snapshot` thread with a shared workspace handle. It publishes the latest `DisplaySnapshot` into a single mutex-protected mailbox. Each full snapshot has a strictly increasing version. `take_pending` clones only a newer version and never blocks on the worker. A dirty flag indicates that a requested pass has not produced a snapshot.

```text
spawn → initial Refresh → seeded snapshot
                   │
       ┌───────────┴───────────┐
       ▼                       ▼
 Refresh (full)       RefreshConversation (cheap)
       │                       │
       ├─ files/tree           ├─ history(cursor)
       ├─ agents/tasks         ├─ coalesce event deltas
       ├─ PTY/processes         └─ update conversation tail
       ├─ LSP/tests/Git/browser
       ├─ workflow/kernel/debug
       └─ trust/graph/revision
                   │
                   ▼
        replace mailbox, clear dirty flag
```

Before the first snapshot, the worker waits for a request. After seeding, it waits up to 250 ms and performs a full refresh on timeout. `request_refresh` coalesces duplicate full-refresh requests with an atomic flag. `request_conversation` coalesces duplicate cheap passes. Conversation events are fetched after a monotonic cursor and rendered from the bounded agent event history instead of rebuilding a session transcript on every terminal frame.

A full refresh projects files (at most 512 displayed file paths), harness availability, typed agent states, agent/task text and evidence, process health, LSP, tests, kernels, debugger, replay, workflows, browser state, Git/GitHub, trust, root, generation, project revision, skills count, tool count, and elapsed duration. Browser supervision marks a previously connected endpoint as `Crashed` when it stops responding; it does not silently restart it.

## Queues, backpressure, and jobs

The worker's internal `ActorRequest` channel is an ordinary multi-producer channel. It is not a fixed-size queue, so callers must use the provided coalescing methods for refreshes. Tool jobs are sent as requests and return a `ToolJobResult`; the worker executes the governed call while holding the workspace ownership path, not in the render task. A job result contains the job ID, tool name, and typed success/error value.

Screenshot requests use a separate visual-request flag. While one screenshot request is pending, additional screenshot requests are ignored. Results contain request ID, requested columns/rows, and a typed result. The worker does not retain an unbounded frame history. Visual capture is explicit and does not make browser pixels authoritative over semantic revisions.

Resident Pi worker channels have separate bounds: command capacity 32, Pi event capacity 256, agent history capacity 512, and per-agent evidence capacity 64. Lossy Pi events may be dropped and the agent snapshot reports `droppedEventCount`; critical ready, failure, stop, and request-start events use the critical path. See [Native Pi SDK runtime](pi-sdk-runtime.md).

Workspace and transport limits remain authoritative: daemon requests and responses are each at most 1 MiB, daemon event batches at most 256, tool result routing at most 512 KiB, project buffers at most 1 MiB, PTY output tails 32 KiB, and managed PTY processes 32 per project. The snapshot worker does not enlarge any of these limits.

## Human interaction model

The render loop applies the newest versioned snapshot and updates local TUI state. It does not perform Git, agent history, browser screenshot, test, or governed-tool work. A slow full refresh is visible through snapshot duration/status while keyboard handling continues. The worker may report a workspace-lock failure or service error in the affected projection; it does not fabricate success.

The development TUI uses these global routes:

| Input | Behavior |
|---|---|
| `?` | Open unified keyboard help |
| `:` | Open the expert command palette |
| `:actions` | Open guided actions for the current surface |
| `Enter` | Open/run the selected item or start/continue Agent composition |
| `Tab` | Move to the next product surface outside editor mode |
| `Ctrl-L` | Open the shared composer dock on the current surface |
| `Ctrl-C` | Open Glass quit confirmation, including from the editor |
| `Ctrl-X` | Abort the selected resident agent |
| `Ctrl-D` | Toggle steer/follow-up mode in the agent composer |
| `[` / `]` | Switch editor buffers |
| `Alt-A` | Prepare an Agent prompt from the focused editor |
| `Alt-W` | Toggle editor soft wrapping |

The full-screen editor renders a header, source block, and footer. It preserves source-line cursor/selection coordinates while mapping the cursor to a visual row. Soft-wrap ON reflows lines to the available cell width and resets horizontal scrolling. Soft-wrap OFF keeps one source line per row and scrolls horizontally. Continuation rows retain gutter width, Unicode cell widths are measured, and an end-of-line cursor is rendered as a cursor cell. The editor starts in INSERT. `Esc` returns to NORMAL. `Esc` from NORMAL on a clean buffer leaves the editor. Unsaved work still asks; it does not silently discard a dirty buffer.

Editor comments, proposals, checkpoints, and explicit collaboration claims are workspace state. The snapshot worker only projects them. Opening or saving a buffer does not automatically create a collaboration claim. Overlapping write claims are rejected only when a caller explicitly invokes the collaboration claim API. See [Development Runtime](development-runtime.md#native-editor-and-conflict-rules).

## Resident agents and tool approval

The TUI submits agent commands to the workspace actor. The resident agent worker multiplexes Pi commands and events; a request acknowledgement is not a settled turn. The UI follows `agent_settled` and typed agent status before showing completion. Mutating tools cross `DevelopmentToolRouter`, which checks workspace trust, actor, generation, project revision, path, confirmation, and browser leases. One native Pi mutation approval is pending at a time. A stale or duplicate approval fails closed. A one-shot non-interactive adapter denies UI requests rather than waiting for an unavailable human.

A tool failure is rendered as an error/evidence result and does not update the project revision as if the tool succeeded. Browser tools disappear while detached or recovering. The worker keeps receiving input while a tool or agent turn is running; cancellation and workspace shutdown are explicit paths.

## PTY and development-suite launch

Project process startup is owned by `ProcessManager`. `glass project run NAME --command COMMAND --wait --root .` is one-shot and requires `--wait`; persistent interactive processes use the TUI `process start NAME COMMAND` or a resident `glass.process.start` call. Detected commands are defaults only when project markers provide them. A real PTY, process-group/Job Object ownership, bounded output, health state, and stop/restart/remove rules apply to both the detected development suite and manually named processes. See [Development Runtime](development-runtime.md#processes-and-ptys).

## External harness discovery, handoff, and delegation

`glass harness list` performs PATH-only discovery. It does not probe versions, use the network, or start a child. The fixed catalog contains `amp`, `aider`, `claude`, `codex`, `gemini`, `goose`, `kiro`, `opencode`, `pi`, `qwen`, `cursor`, and `windsurf`. `glass harness start NAME --root .` resolves a case-insensitive ID, label, or binary and starts that executable in the project directory. It waits for the child to exit, reports exit status, and returns to Glass. A missing executable, unknown name, failed start, or non-zero exit is explicit.

One-shot delegation is a separate non-interactive API:

```console
glass agent delegate codex "inspect the failing test" --root .
glass agent delegate claude - --root . --sandbox read-only < prompt.txt
glass agent delegate opencode "apply the repair" --root . --sandbox workspace-write --allow-mutation --yes --timeout-secs 900
```

Only `codex`, `claude`, and `opencode` are accepted for delegation. The default sandbox is `read-only`. `workspace-write` requires both mutation authority and `--yes` for the exact request, unless `--yolo` is active. Prompt size is capped at 64 KiB, timeout is 1 second to 1 hour (default 600 seconds), stdout is capped at 256 KiB, and stderr at 64 KiB. Codex, Claude, and OpenCode receive harness-specific JSON invocation arguments; prompt and project root remain separate arguments. Glass captures bounded output and returns success, status, timeout, and truncation flags. It never registers the child as a resident agent.

## Shutdown and recovery

Dropping `SnapshotWorker` sends `ShutDown`. It joins the worker when no tool job is marked running. An active job is not used to block the UI teardown path; workspace and process ownership still follow their own shutdown rules. Dropping `AgentRegistry` sends shutdown to every resident worker and joins each owned thread. Daemon shutdown cancels active operations and tasks, clears queued operations, and closes the workspace actor after the active operation completes.

| Condition | Observable result | Recovery |
|---|---|---|
| Refresh worker stopped or channel closed | Requests/results report worker stopped; last frame may remain visible | Reopen the TUI/workspace and request a fresh snapshot |
| Workspace lock unavailable | Affected projection reports lock failure; no fabricated state | Wait for the operation and inspect again |
| Slow refresh | Versioned frame arrives late; duration/status shows delay | Continue input; use the latest frame and inspect the authoritative service |
| Tool/job error | `ToolJobResult` carries an error; no optimistic success | Inspect current state and submit a new governed call |
| Screenshot fails or browser crashes | Typed visual error or `BrowserHealth::Crashed`; semantic workspace remains | Reconnect browser and request a new explicit screenshot |
| Agent event drops | `droppedEventCount` and bounded history show loss | Inspect agent snapshot/history; do not infer omitted events |
| External harness exits non-zero | Handoff returns status failure; Glass workspace remains | Inspect harness output and rerun explicitly |
| Delegation timeout/output truncation | Result marks `timedOut`, `outputTruncated`, or `stderrTruncated` | Narrow prompt/output or choose a bounded timeout; inspect project before retrying |
| Unsaved editor on quit | Exit modal blocks silent discard | Save, or explicitly choose the discard action |

## Verification and evidence

Source-level evidence includes:

- `tui::snapshot::tests::snapshot_worker_publishes_and_applies_without_blocking_ui`, which checks a published complete projection and conversation cursor handling;
- `tui::state` tests for `editor_soft_wrap_toggle_scrolls_visual_rows_and_resets_horizontal_scroll`, Shift selection, editor exit prompts, and responsive input;
- `pi_runtime` tests for native SDK operation mapping, governed tools, unknown-tool recovery, and bounded browser evidence;
- `agents::tests::lossy_worker_queue_reports_dropped_events`, worker panic attribution, cancellation joining, and independent session lifecycle;
- `harness::tests::catalog_covers_requested_harness_families`, case-insensitive resolution, and executable-only discovery; and
- `external_agents::tests` for fixed invocation arguments, read-only OpenCode planning, stream limits, timeout bounds, and workspace-write authority.

These tests prove worker and projection contracts. They do not prove that an installed external harness, provider, browser endpoint, or project command is available on a particular machine. For live behavior, launch the actual TUI or process and inspect its typed state and evidence.

## Related guides

- [Development Runtime](development-runtime.md)
- [Native Pi SDK runtime](pi-sdk-runtime.md)
- [Autonomous task DAGs](task-dag.md)
- [Development TUI architecture](architecture/development-tui.md)
- [Terminal UI architecture](architecture/tui.md)
- [Local daemon](daemon.md)
- [MCP integration](mcp.md)
- [Workspace trust](workspace-trust.md)
