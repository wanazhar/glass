# Durable project threads and run references

## Current behavior

`DevelopmentWorkspace` constructs a new `AgentRegistry` for each open project.
The registry allocates process-local `AgentId` values from `agent-0001`, keeps
agent specs, status, event history, and worker handles in memory, and drops the
records when Glass exits. The default Core runtime already stores conversations
durably and returns a `sessionId` from `session/new`; its protocol also supports
`session/list`, `session/describe`, and `session/resume`. The first ledger
checkpoint stored a Core session reference but did not reconnect it. Exact
reattachment is now specified in the [Core session recovery analysis](issue63-core-session-resume.md).

The live thread-isolation checkpoint groups transcripts and drafts by
`AgentId`. That is correct while one registry is alive, but `AgentId` changes
meaning after a restart and must never become a durable project identity.
Persisted status also cannot establish that a Core process is still running.
Glass must not replay a prompt or queued action merely because a record says it
was active before shutdown.

## Ownership and first checkpoint

Glass owns a project-scoped thread ledger under the already trusted workspace's
`.glass/threads/registry.json`. Each durable thread gets a random `ThreadId`;
each Glass worker incarnation gets a random `RunId`. Core session IDs are
provider-free engine references attached to a Glass thread/run, not thread
identities. `AgentId`, worker/job IDs, credentials, provider configuration, and
claims of live process status are never serialized.

The initial schema is versioned and bounded. It records a workspace fingerprint,
thread ID, title/role, timestamps, selected engine, optional Core session ID,
and bounded run IDs with terminal/recovery classifications. It does not copy
Core transcripts, auth state, pending tool calls, composer drafts, or
follow-up queues. Draft persistence remains pending until TUI edit paths can
flush changes reliably; a schema field alone does not count as implemented
draft recovery.

On Unix, writes use a private `0600` sidecar lock and same-directory temporary
file, flush and sync the complete serialized document, atomically replace the
registry, then sync the parent directory. If that last sync fails, the new
registry is already visible and the error explicitly reports uncertain
durability. Windows files inherit workspace ACLs; this checkpoint does not
strengthen ACL privacy and reopened rows remain unavailable until protected
ACL support exists. Read/parse/schema/workspace-identity/write failures
are surfaced; they never silently replace an unreadable registry with an empty
one. Thread creation persists the durable ID before the runtime starts. The
Core session ID is captured from the initialized runtime, then persisted before
the agent can accept a prompt. If that write fails, the agent remains
unavailable and no prompt is delivered.

On reopen, prior runs with an actual Core session ID are shown as
`recoveryPending`; a run that crashed during initialization or remained in a
dependency queue is shown as unavailable, with queued work identified as never
started. Completed, failed, and cancelled rows retain their terminal
classification. Glass regenerates ephemeral `AgentId` values for all rows and
shows them through the existing Agent picker. A recovered row is not treated as
idle or working. Explicit resume verifies the stored Core session ID, mixed
tool mode, and project cwd before restoring bounded transcript entries and
enabling sends. A failed reattach remains retryable. Glass does not create a
replacement or replay a prior turn.

## State flow

```text
create ThreadId + RunId
        │ persist before spawn
        ▼
initialized Core runtime returns sessionId
        │ persist reference before accepting prompt
        ▼
active in-memory AgentId ── Glass exits/crashes ──► recoveryPending row
                                                       │
                                       explicit verified exact-session resume
                                       transcript restored; no turn replay
```

The first checkpoint proves ledger atomicity/version rejection, captures a real
Core runtime session reference through the adapter boundary, and proves
restart-like registry reopen retains the ThreadId/RunId/session reference and
classifies each row truthfully without dispatching another prompt. It does not yet promise
draft recovery; that remains a follow-up task. Editable follow-up queues and
an attention inbox follow after verified resume.

## Module and integration boundaries

| Boundary | Contract | Evidence |
|---|---|---|
| `thread_store` ↔ project `.glass` | Versioned private ledger; serialized mutations; fail closed on invalid state | Store tests for round-trip, concurrent mutation, schema/workspace mismatch, and write failure |
| `AgentRegistry` ↔ store | Generate stable IDs; persist before worker effects; store actual engine session ID; reconstruct as pending, unavailable, or terminal | Restart-like registry test; no prompt replay |
| `ResidentHarness` ↔ Core runtime | Expose the session ID returned by initialize/open; no synthetic ID | Fake Core runtime adapter test |
| snapshot ↔ TUI Agent picker | Show recovered rows without labelling them live; reject sends while pending recovery | State/projection tests |

## Later issue #63 slices

1. Completed in [the exact-session recovery task](../tasks/issue63-core-session-resume-001.md): describe and resume the exact stored Core session, handle missing or incompatible sessions without fallback, and verify a closed/reopened conversation without a duplicate turn.
2. Persist and edit/reorder/remove follow-up queue entries against stable
   `ThreadId`/`RunId`.
3. Add an attention inbox ordered by durable run lifecycle and link each item
   back to its owning thread.
