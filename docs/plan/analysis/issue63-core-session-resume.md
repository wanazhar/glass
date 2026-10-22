# Exact Core session recovery

## Context

The durable thread ledger gives each Glass conversation a stable `ThreadId`,
each worker incarnation a `RunId`, and stores the real Core session ID. On
workspace reopen, initialized Core runs appear as recovery-pending. The
existing runtime can resume the newest workspace session, but that is unsafe
for a project with multiple threads: recovery must name the session attached
to the selected durable thread.

Core owns its provider configuration, auth, and session journal. Glass stores
only the opaque session reference and Glass-owned thread/run metadata. The
Core session manifest can contain provider, model, endpoint, and credential
environment metadata; Glass must read only `sessionId`, `toolMode`, and `cwd`
for verification and must never copy or render the manifest wholesale.

## Contract

Recovery is an explicit action on an existing recovery-pending row in the
Agent picker. It starts a Core runtime process with the exact stored
`sessionId`, negotiates the existing mixed-tools protocol, calls
`session/resume`, then calls `session/describe`. Before accepting a send, Glass
requires the described session ID to equal the stored ID, `toolMode` to be
`mixed`, and canonical `cwd` to equal the trusted project root. Missing,
closed, malformed, wrong-workspace, or incompatible sessions fail closed;
there is no fallback to `session/new`, no prompt replay, and no silent
replacement ID.

After verification, Glass reads bounded pages from `session/entries` and
hydrates the existing conversation history before publishing Ready. It adds a
new random `RunId` referencing the same Core session, preserving `ThreadId`.
The run becomes usable only after resume verification, transcript hydration,
and durable ledger update have all succeeded. Core session entries are
projected as transcript events with their Core entry IDs; metadata and
credentials are excluded.

Recovery is host-process-specific. The runtime process created by this
checkpoint is the one resumed; no other live Core service is interrupted or
restarted. If a stored session cannot be opened, the recovery attempt fails and
the row remains visible with its prior durable reference and an actionable
error. A later explicit retry may attempt the same session ID.

## Implementation boundaries

| Area | Change |
|---|---|
| Core adapter | Accept an explicit resume session ID; verify `session/describe`; load bounded `session/entries` pages before returning Ready; never fall through to create |
| Resident harness | Carry explicit recovery ID and startup transcript without introducing a second engine-specific persistence layer |
| Agent registry | Explicitly recover only recovery-pending Core rows; append a new RunId with the same Core session ID; keep sends blocked until the worker reports verified startup |
| Glass tool and command | Add `glass.agent.resume` and `:agent resume [AGENT_ID]`; target a selected row without changing focus implicitly |
| TUI | Restore transcript entries into the selected thread's event history before Ready; retain existing picker and thread ownership |
| Tests | Fake Core restart journey proves exact ID, manifest checks, transcript hydration before Ready, no `session/new`, and no `turn/prompt` replay; reject missing ID, wrong tool mode, wrong cwd, and describe mismatch |

The checkpoint does not persist drafts or queues, alter provider/auth
ownership, change Pi legacy behavior, introduce a new picker, or resume any
Core session implicitly. `AgentId` remains process-local; durable identity is
always the Glass `ThreadId` and actual Core session reference.
