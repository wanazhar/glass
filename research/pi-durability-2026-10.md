# pi durability survey — what to borrow

Compiled 2026-10-08 from primary sources: a clone of `github.com/earendil-works/pi`
at tag `v1.1.0` (release 2026-10-07), its in-tree docs and packages, and the
Program Status Protocol spec. Every claim cites a repo path or URL. Anything
unconfirmed is marked **UNVERIFIED**. Follow-up to
`harness-features-2026.md` and `harness-delta-2026-10.md`; this one is
single-harness and narrowly about durability.

## Headline

pi has **two persistence systems**, and only one is durable:

- The **shipped CLI** (`@earendil-works/pi-coding-agent`) writes an
  append-only JSONL session log — one `appendFileSync` per finalised message,
  no fsync, no sequence numbers, no repair pass, no record of an unfinished
  turn. A `kill -9` mid-turn loses the whole in-flight response. On most
  durability axes this is *weaker than glass-core already is*.
- **`packages/durable`** (`@earendil-works/pi-durable`, "API changes without
  notice") is where the engineering is: a task scheduler, an effect/intent
  protocol, a commit-marker WAL, throttled partial commits, and a document
  store. The published CLI **does not depend on it**
  (`packages/coding-agent/package.json`); it is wired only into
  `src/experimental/{durable,vacation}`.

License: **MIT, © 2025 Mario Zechner** (`LICENSE`, every package's
`package.json`). Reimplementing concepts is unencumbered; glass-core is Rust,
so we reimplement, never copy.

## What pi is (context)

- TypeScript ESM monorepo, 14 packages, Node ≥ 22.19; one CLI with TUI,
  `--print`, `--mode json`, `--mode rpc`; a separate SDK. MIT.
- 42 provider factories (`packages/ai/src/providers/all.ts`); generated model
  catalog.
- Extensions are in-process TypeScript modules (`jiti`, no build step) with
  ~40 lifecycle events, tool-exposure modes (`direct|model-only|codemode|
  deferred|hidden`), and a package manifest (`docs/extensions.md`,
  `docs/packages.md`). No built-in sandbox or permission system — isolation is
  delegated to Docker/OpenShell/a QEMU micro-VM (`docs/security.md`).
- No subagents, no plan mode (deliberate; both exist as examples).
- MCP: stdio + Streamable HTTP (SSE rejected), OAuth incl. CIMD, project
  overrides (`docs/mcp.md`).

## Durability findings (the focus)

### Shipped session log

- `~/.pi/agent/sessions/--<path>--/<ts>_<id>.jsonl`; header line + entries
  with `{type,id,parentId,timestamp}`; entries form a tree via `id/parentId`,
  so branching is in-place (`docs/session-format.md`).
- Persistence is append-per-finalised-message; the prompt is persisted from
  the first append, explicitly so an unfinished first turn is still on disk
  (`packages/coding-agent/src/core/session-manager.ts`). Compaction rewrites
  the file when materialising a fork; otherwise append-only, no checksums.
- **No repair on load** (parse → migrate → index). **No in-flight turn
  record.** Crash diagnostics land in `~/.pi/agent/crashes.json`
  (`src/core/crash-log.ts`) — useful, not recovery.
- Workspace state: **none**; the only file checkpoint in the repo is an
  *example extension* using `git stash create`. glass-core is ahead here
  (git checkpoints + undo).

### `pi-durable` mechanisms

- **Effect sandwich** (`packages/durable/src/harness/tool.ts`): before
  executing, one commit writes the intent —
  `{phase:"execute", arguments, replay: "safe"|"unsafe"}` (default unsafe).
  Recovery reaches the `execute` phase: rerun only if the stored *and* the
  current policy are both `safe` (after clearing published progress);
  otherwise append an interrupted result carrying the durable partial output:
  *"Tool X was interrupted and may have partially run"*.
- **Commit-marker WAL** (`src/storage/jsonl/storage.ts`): sidecar records for
  tables/documents/tasks, then one `main.jsonl` marker line per commit is the
  commit point. Open: require increasing `seq`; truncate a torn final line;
  truncate unconfirmed sidecar tails; confirmed-but-missing data is
  `JsonlCorruptionError` (fail closed); any uncertain append poisons the
  storage until reopen. `fsync` is opt-in and documented as the difference
  between process-crash consistency and power-loss survival.
- **Task scheduler** (`src/harness/scheduler.ts`): on open, surviving
  `running` tasks are rewritten to `pending` in one commit, carrying their
  checkpoints; reconcile re-derives abort/fail-fast/terminal records from
  committed state.
- **Throttled partials** (`generation.ts`): the in-flight response is
  committed at most every `progress.partialIntervalMs` (default 100 ms), one
  commit in flight, and the `finally` awaits it "so no stale partial lands
  after the outcome".
- **Idempotent submissions**: `submit({requestId})` returns the existing
  submission for a repeated id; `harness.submission(id)` reacquires one.
- **Watching**: late joiners get the current view, nothing replayed; >100
  pending frames collapse into one newest-view reset.

## Borrowability

| pi mechanism | Verdict | Note |
|---|---|---|
| Effect sandwich + replay policy | **borrow** | Maps to one record + an open-time repair in glass-core. |
| WAL repair semantics | **borrow the repair half** | Torn-tail truncation, fail-visible corruption. Marker lines: no (below). |
| Task records / scheduler | **borrow the intent, not the machine** | We show "interrupted" + `/retry`; no unattended auto-resume. |
| Throttled partials | **borrow** | A `<session>.live.json` sidecar fits our existing sidecar pattern. |
| Idempotent `requestId` | **borrow** | In-process dedupe on the serve protocol. |
| Durable retry clock | **borrow small** | Persist attempt/next-at in the live sidecar. |
| Chord documents / task graph | **skip** | 20k lines of TS-async framework for guarantees we get from ordering. |
| Cloudflare DO backend | **skip** | Platform-specific. |
| Session tree (in-place branching) | **skip** | glass-core branches by forking files; the tree is pi-specific UI. |

### Deliberate deviations, with reasons

- **No commit-marker lines.** Every multi-record invariant glass-core has is
  ordering-safe: intent precedes effect (a crash between them *is* the
  interrupted case), the compaction marker is a single trailing record, state
  is one record. Markers would double log size for no atomic group that
  needs them. We take the *repair* semantics instead.
- **Corruption is fail-visible, not fail-closed.** pi can afford fail-closed
  because markers make "confirmed data missing" unambiguous. Our logs are
  plain append-only JSONL; refusing to open a session over one bad byte
  locks a user out of their work. Instead: torn tail → truncate + warn;
  other unparseable lines → back up the original, drop the line, append a
  visible `Meta` record naming the backup.
- **No auto-resume.** pi resumes interrupted tasks inside a live run.
  glass-core surfaces the interruption and lets the user spend the tokens
  via `/retry` — unattended re-execution is a destructive-action risk we
  will not take silently.

## Borrowed in this batch

1. Log integrity: torn-tail repair, visible corruption with backup, `log_fsync`
   policy (`session.rs`).
2. Effect sandwich: `Record::Intent` before execution, interrupted-result
   repair on open keyed by replay policy, never re-run, never a dangling
   `tool_use` (`session.rs`, `agent.rs`).
3. Live sidecar: throttled partial commits (250 ms default) with tool output
   and retry state, recovered on open as a visible partial
   (`live.rs`, `agent.rs`).
4. Cursor-based attach (`since` + `cursor`) and `request_id` dedupe
   (`serve.rs`).
5. `context_edit`: append-only `/forget`, `/rewrite`, `/unforget` that change
   only future model context (`session.rs`, `main.rs`).
6. OSC 7501 program status: working/blocked/done/error/idle with feature
   detection (`status.rs`, `input.rs`).

## Method and limits

Primary sources only: repo clone at v1.1.0, in-tree docs (`docs/session-format.md`,
`docs/terminal-setup.md`, `docs/mcp.md`, `docs/extensions.md`), package sources
(`packages/durable/src/harness/{tool,generation,scheduler}.ts`,
`packages/durable/src/storage/jsonl/storage.ts`), GitHub release metadata, and
the Program Status Protocol spec at superlogical.com/rex/docs/build/program-status.
No access to pi's issue tracker beyond references in code comments. Version
drift is possible; the durable package explicitly ships no compatibility
promise.
