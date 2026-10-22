# Workbench workflows

[Documentation index](../INDEX.md) · [Workflow guide](../workflows.md)

GC keeps optional services off its startup path. These commands use local
metadata and shared native modals; inspecting a panel does not ask the model.
Use `gc help all` for command discovery and `/keys` for terminal controls.

## Conversations and context

- `/hub` is an alias for Work. `/sessions` opens the shared conversation chooser,
  with 64 rows per page, global title/ID/project search, and all-project scope.
  The separate `gc hub --json` snapshot remains capped at 512 metadata rows
  and reports omissions. Unread counts refer to recorded events.
- `/branches` uses the shared chooser's branch-family view. Forking retains the
  original conversation and leaves workspace files unchanged.
- `/quote` lets you select a retained message, preview/edit an excerpt, and attach
  it with its source session, record and fingerprint. `/context` removes it.
  File ranges retain their original path, line range and revision citation.
- `gc history ID --range START COUNT --json` reads retained display entries,
  including history omitted from the active model projection. COUNT is 1–256;
  each page has a 64 KiB text budget. Follow `next_entry` for another page.
- `/history` includes compaction boundaries, summary request usage when reported,
  retained-history references and cumulative shell-observed file hints. File
  hints are not complete filesystem tracing. Missing usage is unknown.
- `/handoff` previews, saves and imports bounded portable context. CLI import is
  preview-first: `gc handoff import FILE --json`, then `--apply` to create a new
  conversation. Import neither runs the model nor transfers tool permissions,
  credentials, files or live processes. Existing export files are preserved.

## Work and review

- `/queue` manages durable follow-ups; Enter during work steers, Alt+Enter queues.
  Delivery receipts describe acceptance separately from provider completion.
- `/attention` collects recorded failures, team events, open findings, explicit
  handoff questions and current-process owned job completions/review checkpoints.
  Findings have exact IDs and source locations. Completed jobs open their live
  output inspector. CLI attention cannot inspect another process's live job registry.
- `/activity` opens session-owned background jobs. Their inspector follows bounded
  output, scrolls, accepts stdin with `I`, and sends EOF with Ctrl+D. Escape closes
  the inspector without stopping the job; reopening reconnects to the same job.
  Jobs are owned by the GC runtime, not a daemon that survives runtime shutdown.
- `/turns` reviews observed checkpoint intervals; it does not assert exclusive
  agent authorship. `/rewind` previews restoration and offers keep-files mode.
- `gc worktree new NAME [BASE] --branch BRANCH` creates a named task branch in an
  isolated checkout. Omit `--branch` for the compatible detached workflow.
  `show NAME` supports inspection/reuse; `cleanup-preview NAME` never deletes.
- `/pr` opens GitHub status, CI checks, review feedback and draft preparation.
  GitHub CLI is invoked only on selection. CLI publication requires the exact
  preview fingerprint:

  ```sh
  gc pr preview --title "Task summary" --body-file pr.md --json
  gc pr create --publish --expect-hash PREVIEW_FINGERPRINT \
    --title "Task summary" --body-file pr.md
  ```

  Push explicitly first. Publication checks the current branch/commit and local
  upstream commit; it does not push or merge. Network failures can be ambiguous:
  inspect GitHub state before retrying a publish.
- `/advisor` uses the existing persistent reviewer runtime. Configure a reviewer
  role first with `gc workspace role set reviewer MODEL TOKENS`. Reviews are
  manual by default; opt into a completed-turn cadence in the modal or with
  `gc advisor SESSION cadence N`. Unchanged tracked patches are deduplicated.
  Usage and dispositions are separate from the main model's work. Untracked
  files require staging before they are included in the tracked patch review.

## Automations and optional clients

`/loop 30m check progress` binds a schedule to the current host, project and
model. A mismatch parks it. `/loop bind N` deliberately rebinds a schedule;
legacy unbound schedules also require this action. `/loop missed N once|skip`
chooses catch-up policy. `/loop history` records bounded dispatch history;
completion belongs to the conversation. Acceptance is durable before the
schedule advances and a crash cannot blindly replay an accepted interval.

`gc companion SOCKET [PORT]` starts the optional loopback browser client for a
running `gc serve` socket. The private URL authorizes access. It starts as an
observer; claim input explicitly. Reconnect returns to observer mode. Streaming
text coalesces into bounded blocks. GC owns provider calls and all effects.
Use an explicitly configured SSH tunnel for access from another device.

External ACP harnesses and DAP debuggers use the optional bundled
`extensions/adapters/adapters.py` extension. Enable that extension explicitly
and configure `~/.config/glass-core/adapters.json`, for example:

```json
{
  "debugger": {
    "protocol": "dap",
    "command": ["/absolute/path/to/debug-adapter"],
    "allow_debug_execution": false
  },
  "harness": {
    "protocol": "acp",
    "version": 1,
    "command": ["/absolute/path/to/acp-harness"],
    "allow_agent_execution": false
  }
}
```

`/adapters` discovers names without starting processes. The `adapter` extension
operation starts only a configured name, returns request tickets, and exposes
initialization capabilities through bounded polling. Execution needs its
separate explicit flag; unsupported reverse execution requests are refused.
Disable the extension to release its owned adapters. Core credentials are not
forwarded. `gc structure --help` exposes optional bounded AST transformations
with previews and stale-checked transaction application.

Persistent `gc serve` sessions also own the teammate hub and run opted-in advisor
cadence after successful turns. One-shot `gc run` records navigation metadata;
it does not leave a background reviewer running after its process exits.
