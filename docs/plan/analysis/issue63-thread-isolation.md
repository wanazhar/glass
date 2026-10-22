# Live resident-agent thread isolation

## Current behavior

The Glass TUI selects a resident agent by `AgentId`, but `SnapshotWorker`
currently reads the whole `AgentRegistry` event history and flattens messages
from every agent into one `conversation_items` projection. The composer has
one `composer_input` and cursor for the whole TUI. Switching the selected
agent changes the command target but does not select a matching transcript or
preserve a separate draft.

Chat sends are asynchronous. A late result currently can select the agent
named by the response, replace the visible status, or copy a failed message
into whichever composer is currently open. That can move focus or overwrite a
draft after the user has switched to another resident agent.

## Contract for this slice

For each live resident agent, the TUI treats its AgentId as a temporary
thread key. Conversation events are grouped by their owner AgentId, and the
selected thread displays only its own transcript. Each live thread keeps its
own composer text and character cursor while the user switches agents. The
unthreaded composer remains available before an agent is selected.

```text
Agent A selected ── type draft A ── switch ──> save (A, text, cursor)
                                           └─> load (B, text, cursor)

late result for A ──> update A's pending message/transcript
                  └─> do not select A, change B's composer, or steal focus
```

Every pending send records its owning AgentId at submission. A result or
failure updates that message and owner only. A failed message remains
recoverable in the originating thread's composer without replacing another
thread's current draft. Selection changes stay explicit user actions.

## Scope boundary

AgentId is allocated by the current in-memory `AgentRegistry`; it is not a
durable project thread identity. This slice does not persist drafts across TUI
restart, persist AgentSpec/run metadata, or restore Core sessions. It does not
make Core session files interchangeable with Glass project threads. A later
slice must add Glass-owned durable thread/run references and recovery before
restart durability can be claimed.

This slice also does not add an editable follow-up queue or a new attention
inbox. It keeps the current Core request ordering and the existing agent
activity surfaces.

## Ownership and verification

| Module | Responsibility | Focused evidence |
|---|---|---|
| `tui/snapshot.rs` | Group event history into per-AgentId conversation projections | Two agents' events remain in separate projections |
| `tui/state.rs` | Preserve/restore per-thread draft and cursor; retain owner on pending sends | A/B switch restores each draft and cursor; failure remains on origin |
| `tui/command.rs` / `tui/mod.rs` | Explicit selection and asynchronous result behavior | Late A result does not change selected B or its composer |
| `tui/projection.rs` | Preserve existing message/tool formatting within each thread | Same projection contracts for each independent event stream |

Tests must exercise two live agent identities with distinct transcripts and
drafts, select each thread, and deliver a late result to the first after the
second is selected. The regression must prove transcript, pending-message,
cursor, and focus ownership. Run focused `glass-dev` tests, scoped Clippy,
formatting, and current documentation validators. Reuse the shared Cargo
target; do not push or release.
