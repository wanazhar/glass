# Conversations and context

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Sessions

One append-only JSONL file per session under `$XDG_DATA_HOME/glass-core`. The same
file is the crash-recovery mechanism, the resume mechanism, and the source for
the transcript viewer — which is what keeps the two renderers from drifting.

```sh
glass-core list          # sessions, newest first, with date and title/first message
glass-core search parser # sessions whose messages match the pattern
glass-core resume
glass-core resume SESSION_ID
```

Prompt history is persisted too: `up` recalls prompts from earlier
sessions, capped at 200.

## Conversations

`/sessions`, `/branches`, and `gc resume --browse` use the same metadata-first
chooser. Lists show 64 conversations per page. Use older/newer pages, all-project
scope, or the explicit search across indexed titles, IDs, and project paths.
Typing in the ordinary list searches the current page.

Selecting a conversation opens its bounded preview, model/status receipt, lineage,
and unread recorded-event count. Unread events are not a message count. Previewing
does not change the read cursor or invoke the model.

| Key | Action |
| --- | --- |
| `r` | Resume |
| `p` | Open the shared transcript reader |
| `n` | Rename |
| `f` | Fork through a selected turn |
| `s` | Save a follow-up to the target inbox |
| `d` | Delete after confirmation |

The current conversation and any conversation held by another writer cannot be
deleted. Workspace files and checkpoint snapshots are retained. In-app resume or
fork of another project's conversation requires switching workspace. CLI selection
can switch to the chosen workspace.

`gc resume` resumes the selected row directly. `gc resume ID` opens a known ID,
including older sessions absent from the index. `gc resume --browse` opens details
first. Directory indexing reads bounded metadata, with a bounded first-message
fallback for untitled sessions; it does not enumerate full transcripts.

## Sessions, context, and task review

`/sessions` searches conversations in the current project. Choose a session to
preview, rename, resume, branch through a chosen turn, or delete with confirmation.
The current session cannot be deleted. Deletion removes its transcript, title,
attachments, and cost ledger; workspace files and checkpoint snapshots stay.
Sessions are bound to the canonical workspace path. Older logs acquire a project
binding when explicitly resumed, and then appear in that project's browser.

`/name <title>` sets the title directly; `/name` opens a modal prefilled with the
current title, and an empty field falls back to the first message. `glass-core
list` shows titles, and so does `/sessions`.

Resuming a conversation shows a themed **welcome-back recap** with the latest
request, a saved reply excerpt, and any unresolved error. `/recap` reopens it in a
scrollable modal (Enter/Escape closes). It is generated locally and cached outside
model history: **zero extra tokens**, no provider request, and no automatic tool
execution. An excerpt reports saved text, not a new verification of completed work.

`/context` shows the last foreground model request and a browser of past requests,
plus current prompt configuration, attachments, context edits, and compaction.
Request manifests record instruction/attachment paths and byte counts, available
tools and schema sizes, history counts, and which skill descriptions were supplied.
Skill bodies are loaded on demand through tools; inspect `/details` for commands
and results. Manifests describe supplied context, not proof the model read it.
They are saved as metadata in a private `.context` sidecar beside the session log,
never added to model history or compaction. Older sessions lack these manifests.
Headless access: `glass-core context SESSION_ID --json`.

The footer labels turn **input/output** separately from **session total tokens**
accumulated during this run. Context fill uses the latest model request input,
not summed usage across tool rounds. Manifest token estimates include tool schemas
and a rough image allowance; provider-reported usage remains authoritative.
`/attach` opens a searchable project file picker; `/attach path` also works.
Text attachments are saved UTF-8 snapshots, capped at 32 KiB per file and
128 KiB in total. Images (PNG, JPEG, GIF, WebP) attach as pictures instead:
up to 1 MiB each and 2 MiB in total, relayed to the model as image blocks —
no codec decodes them, they are bytes passed through — and shown in the
transcript as `[image: png · N KB]`. They become part of the conversation
log, so a resumed session still has them. Remove attachments in `/context`;
removing one changes future prompt context, and does not remove material
already present in conversation history.

## Crash recovery

Opening a session checks its append-only JSONL log. A torn final line is removed
before further appends. Complete invalid JSON records are backed up and repaired
with a visible notice. Unknown records from newer versions are preserved.
`log_fsync = "always"` requests syncing on each append; the default is `"off"`.

Calls with no recorded result receive an interrupted result on resume. They are
not automatically rerun: a shell operation may already have changed the workspace.
Inspect the affected files before retrying uncertain work.

During streaming, partial text, active tool information, and retry state are saved
in a `<session>.live.json` sidecar. `live_flush_ms` defaults to 250; zero disables
periodic persistence. Normal completion removes the sidecar. Recovery preserves
available partial output and identifies interrupted work.

Attach clients use durable submission IDs to reconcile reconnects. Recovery
receipts describe what was recorded; they cannot verify an external operation
that completed without a result reaching the log.

## Context edits

Context edits change the projection sent to future model requests while retaining
the original transcript and files:

```text
/forget 3
/rewrite 3 condensed wording
/unforget 3
```

The latest edit for a turn takes precedence. `/forget` without an argument lists
turns and their edit state. `/context` shows active edits. Raw exports retain the
original history. Use workspace checkpoints separately to restore files.

## Context management

Compaction reduces model context while retaining the durable conversation log.
The runtime first uses deterministic reduction, preserving the opening task,
current state notes, and recent turns. If further reduction is needed, it can
request a model summary. Summary requests consume provider quota.

`/compact [focus]` requests compaction explicitly; `/compaction` is an alias.
Summary input is bounded per record. If summarization fails, deterministic
recovery remains available. Repeated summary failures suspend further paid
summary attempts for the session through a circuit breaker.

Provider context-overflow recovery reduces oversized results and history before
retrying. Replaced results carry explicit placeholders and retrieval references.
Completed tool identities remain recorded so reduction does not turn completed
work into a new execution request. `/history` exposes compaction receipts and
retained-history references.

## Conversation branches

`/branch` forks the current conversation into a new session; `/branch 3` forks
through user turn 3. The original log stays intact. Branching reads the complete
log, so it works after compaction. Conversation branching keeps the workspace
files as they currently are; use a checkpoint restore separately when desired.
Each branch starts its own spending ledger with the current limit.

## Side questions

`/btw QUESTION` requests a tool-free side answer using a bounded context snapshot.
Its prompt and answer are not appended to the main conversation. It still consumes
provider quota and is metered through the session ledger. Provider caching and
usage can vary; keeping the side exchange out of history does not guarantee a
particular cost for the next request.

## Provider retry boundaries

GC automatically backs off on explicit HTTP 429 and 5xx responses before a
stream starts. A network failure can happen after the provider accepts a POST,
so ambiguous transport errors require an explicit `/retry` and do not trigger
automatic model fallback. Automatic HTTP
retries also depend on provider billing policy; they are not guaranteed free.
Completed local tools are not replayed by this connection retry loop.
