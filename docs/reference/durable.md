# Native durable runtime

[Documentation index](../INDEX.md) · [Host protocol](runtime.md)

The optional durable API runs on GC's Rust engine through `gc runtime` or its
`gc durable` alias. It does not require Node.js, a browser, a database service,
or a rendering process. Ordinary chat and host sessions keep their existing
storage path. The durable journal opens on the first `durable/*` request.

This implementation follows the execution contracts in
[Pi Durable](https://github.com/earendil-works/pi/tree/main/packages/durable),
reviewed at commit `5ab7fe74c400097b386662205cd93900695b8a71`. It uses GC's
protocol and extension interfaces, rather than exposing Pi's TypeScript API.
Pi's package is experimental; compatibility refers to behavior, not package names.

## Starting and resuming work

Initialize the host protocol as usual, supplying an absolute `storageDir`.
Create a conversation with `durable/create`, using the same provider, working
directory, instructions, and tool options as `session/new`. Then submit input:

```json
{"method":"durable/submit","params":{"conversationId":"ID","requestId":"greeting-1","input":{"text":"Hello"}}}
```

The result contains `submission.id` and `duplicate`. Repeating the same request
ID and input returns the existing task, including after a restart. Different
input with that ID is an error. Input remains queued until execution starts;
generations within a conversation run in submission order.

Call `durable/run` with a `taskId`, or `durable/resume` without one to run ready
generations. Each call processes at most 32 generations and reports the settled
task IDs. This is a host-controlled scheduling boundary, not a model tool-call
budget. Calling it again continues the queue. The process does not start model
requests merely because storage was reopened.

`tool/execute`, tool progress, cancellation acknowledgements, and `runtime/event`
use the existing host protocol. Committed tool intent precedes execution.
Completed tool results remain in context across restart. An interrupted unsafe
tool receives an explicit reconciliation error; GC does not silently repeat it.
This provides at-most-once automatic dispatch, not exactly-once external effects.
The host must reconcile uncertain remote effects before retrying them.

A disconnected generation remains pending. Explicit abort remains abort across
restart. A partial answer committed before disconnect is retained. Uncommitted
stream fragments can be lost; live updates are committed in batches at roughly
100 ms intervals and at phase boundaries.

## Atomic state and custom tasks

`durable/commit` accepts `expectedSequence` and `operations`. One checked journal
frame commits all operations or none. A sequence or document/task revision
conflict changes neither storage nor published state.

| Operation | Required fields beyond `op` |
| --- | --- |
| `entry.append` | `conversation`, `record` |
| `document.put` | `conversation`, `kind`, `version`, `expectedRevision`, `value` |
| `document.migrate` | `conversation`, `kind`, `version`, `expectedRevision`, `value` |
| `task.create` | `id`, `conversation`, `kind`, `version` |
| `task.update` | `id`, `expectedRevision`, `status` |
| `task.migrate` | `id`, `expectedRevision`, newer `version`, `checkpoint` |
| `task.abort` | `id`; optional `includeBackground` |

Document definitions optionally set `history` and `fork` (`initial`, `current`,
or `as_of`). Definitions remain stable until an explicit migration changes the
version/value. Historical reads require `history: true`.

A task can set `owner` to its parent task ID, `background`, `input`, `checkpoint`,
and `replay` (`unsafe` by default). Register current custom task definitions with
`durable/register`, supplying `tasks: [{kind, version, replay}]`. Definitions
are process-local and must be registered after reopening. Missing definitions,
version mismatches, or an incompatible replay policy block dispatch.

`durable/claim` claims a ready custom task; the host executes its phase and saves
its checkpoint using `task.update`. GC executes its built-in generation/tool
tasks; custom application code stays with the host. A running task may checkpoint
without becoming terminal. Set `waiting` with `on` task IDs and a `policy` of
`all_settled` or `fail_fast`. Fail-fast dependencies must be owned children.
Wait cycles are rejected. A task with foreground children remains `completing`
until they settle. Background children survive a parent's ordinary abort;
`includeBackground: true` includes them.

Safe interrupted tasks return to pending. Unsafe tasks become failed with an
interrupted outcome. Mark work safe only when repeating its phase is safe.
Migrations require a pending task, matching revision, and newer version.

## Inspection and observation

| Method | Purpose |
| --- | --- |
| `durable/task` | Get one task by `taskId`, including checkpoint and outcome |
| `durable/submission` | Reacquire by `conversationId` and `requestId` |
| `durable/inspect` | Page conversations, documents, and tasks; optional `conversationId` |
| `durable/changes` | Read atomic commits after a numeric sequence `cursor` |
| `durable/taskGraph` | Inspect foreground/background ownership of live tasks |
| `durable/document` | Read by `conversationId`, `kind`, optional `asOf` sequence |
| `durable/entries` | Page the active transcript by numeric record cursor |
| `durable/configure` | Merge an `agent` settings patch into a conversation |
| `durable/fork` | Fork at optional `asOf`; document fork policies apply |
| `durable/reset` | Reset active context once all conversation tasks have settled |
| `durable/abort` | Request task abort; optional `includeBackground` |

Inspection returns `items`, a string `cursor`, and `hasMore`; continue with the
returned cursor. Changes return `commits`, numeric `cursor`, and `hasMore`.
A commit too large for one observation response produces `reset: true`; reacquire
state through inspection and direct task/document reads. Observation does not
allocate an unbounded subscriber queue. Polling can resume from the last cursor.
Changes, task graph inspection, and abort work while a generation is running.
Other requests currently require the runtime to be idle.

Each conversation has a persisted provider identity. Reopen, reset, configuration
changes, and compaction retain it; forks receive a fresh identity. It is forwarded
as session-affinity headers. The OpenAI prompt-cache body field is sent only to
the official OpenAI API, avoiding unsupported fields on compatible endpoints.

## Storage and performance

`storageDir/durable/durable.jsonl` is authoritative. Each frame has a sequence
and chained SHA-256 integrity check. A torn final frame is discarded; committed
corruption fails closed. A single-writer lock prevents concurrent writers.
An uncertain append poisons the writer until reopen. `log_fsync = "always"`
adds a data sync at every commit; without it, process-crash recovery is supported,
but power-loss durability depends on the operating system and storage.

`durable-projections/*.jsonl` are derived session projections, rebuilt from
committed records before execution. They are not a second source of truth.
Terminal tasks and deduplication receipts remain available; input bodies are
removed from resident generation tasks after admission to context. The journal
retains the original submission. There is no automatic deletion of durable data.

Frames are limited to 1 MiB and 512 operations; resident metadata to 32 MiB and
16,384 objects. Inspection pages contain at most 128 resources, change pages 100
commits, and responses use a 768 KiB payload budget. Entry reads currently scan
history, and commits clone current metadata for validation. Large installations
should use separate storage directories for independent workloads. Automatic
archival and an indexed backend remain future optimizations.

The native contract does not include Pi's Node, SQLite, Cloudflare, JavaScript
code-mode, or Chord APIs. GC retains its own extensions, tools, approvals,
compaction, and provider engine. Concurrent model turns in one runtime process
are not supported; independent processes can use separate storage directories.
