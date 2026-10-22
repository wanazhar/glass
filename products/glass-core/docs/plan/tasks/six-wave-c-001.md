---
id: six-wave-c-001
scope: wave C — checkpoint/rewind tools, wait tool, prewalk handoff, branch summarization
status: ready
source: research/six-codebases-2026-10-10.md candidates #12, #13, #14, #11
---

# Wave C: context management

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

1. **Checkpoint → rewind tool pair** (OMP tools, default off):
   `checkpoint.enabled = true` (config) registers two model tools.
   `checkpoint` marks the current turn boundary durably (a new
   `Record::Checkpoint { label, log_len }`, validated label ≤ 80 bytes).
   `rewind` names a checkpoint and ends it: the log keeps every record, but
   the model-visible projection drops post-checkpoint assistant/tool
   content and inserts a bounded retained report the model writes inside
   the rewind call (`report` argument, ≤ 4 KiB). File changes are NOT
   reverted — receipt says so explicitly; `/rewind` (user command) already
   owns file restoration and this tool never touches it. One active
   checkpoint; rewind without one fails with the reread-style hint.
2. **Wait tool** (OMP `wait`): extension tool `wait` with optional seconds
   (default 60, cap 300). Blocks the *turn* on the existing attention/event
   source until a background result, teammate message, or loop dispatch
   lands, then returns the summary line. Timeout returns "no event";
   cancellation via the existing steer path. Not a busy poll: reuse the
   existing recv machinery. Never appear in read-only/plan mode.
3. **Prewalk** (OMP prewalk): `prewalk = "smol"` config with a model id.
   When set and a turn ends with a pending todo list and no failing tool
   state, `/prewalk next` (user command, and headless
   `gc prewalk --check` flag) hands the session to the target model:
   writes one `Record::System` noting the handoff, records the answering
   model, and the next turn routes to the target. The planning model
   wrote the plan; the cheap model executes. Confirmed by an explicit
   user action; never automatic mid-turn.
4. **Branch summarization** (pi branch summarization): when a resumed
   session's projection skips more than 60 records between the last user
   message and the compaction boundary, /resume — and the first turn after
   — gains a bounded branch summary line listing retained files touched
   and last assistant action. Reuses the compaction summarizer disabled
   for prompt-cache writes, per pi's rule (one-off summary never writes
   cache).

## Path

Own these paths only.

- `crates/glass/src/session.rs` — `Record::Checkpoint`, projection rules
- `crates/glass/src/tools.rs` — checkpoint/rewind/wait tool dispatch
- `crates/glass/src/provider/mod.rs` — tool schema entries only
- new `crates/glass/src/checkpoint_tools.rs`, `crates/glass/src/wait_tool.rs`
- `crates/glass/src/compact.rs` — branch summary helper
- `crates/glass/src/main.rs` — `/prewalk` command and `gc prewalk`
- `crates/glass/src/config.rs` — checkpoint.enabled, prewalk target
- new `scripts/smoke-checkpoint-rewind.py`, `scripts/smoke-wait.py`
- `docs/plan/tasks/six-wave-c-001.md`

Do not edit `agent.rs`, `ext/**`, `subagent.rs`, `attention.rs`,
`readguard.rs`; other waves own those.

## Constraints

- All three tools are bounded: checkpoint labels capped, report capped,
  wait timeout capped; bounds stated in output.
- Extension tools only: checkpoint/rewind/wait do not alter the
  two-tool default when the feature flag is off; with it on they appear
  via the extension tool path (config-gated registration), preserving the
  byte-stable tool list until session start.
- The projection drop must be replayable: old logs without
  `Record::Checkpoint` restore unchanged (hard compatibility).
- rewind must never mark a completed mutation as reverted; consistency
  with recovery inspector's no-replay rule.
- Wait returns the same attention summary rows the inspector uses; no
  second event taxonomy.

## Verification

- Unit tests: checkpoint round-trip and projection drop; rewind without
  checkpoint refused; report bounded and kept; wait timeout path; wait
  woken by a queued event; prewalk requires explicit action; branch
  summary bound; legacy logs restore.
- `scripts/smoke-checkpoint-rewind.py` drives both tools headlessly;
  `scripts/smoke-wait.py` proves the wake path and cap.
- clippy + full tests green.
- PTY: `/tools` lists the two new cards with the shared card presenter;
  rewind projection visible in history at 40×24; 20×8 shows the focusable
  detail row per existing dialog rules.

## Shipped (2026-10-11)

- `Record::Checkpoint { label, log_len, rewind_summary }` in session.rs with
  replay-safe serialization; open checkpoint hides later records from the
  projection, a rewind injects the retained report as a system note.
- `checkpoint_tools.rs`: `checkpoint` (label ≤ 80 B), `rewind` (report ≤ 4 KiB,
  start/end preserved with the middle elided but full text noted), `wait`
  (default 60 s, cap 300 s, polls the attention summary rows — same taxonomy,
  no second event source).
- Schemas registered via `provider::checkpoint_tool_schemas`, gated by
  `checkpoint_tools = true` (zero schema bytes when off). Dispatch in
  agent.rs `run_tool` (fixed: the branch was nested under `state`).
- Prewalk: `prewalk = "MODEL"` config, `/prewalk` `/prewalk off` commands,
  one durable `Record::System` handoff note, next turn routes via
  `runtime_config`; headless `gc prewalk --check [--json] [SESSION]` reports
  the read-only gate (pending plan, failing tail, target configured).
- Branch summarization: `compact::BRANCH_SUMMARY_GAP` (60) with an extractive
  `branch_summary` (last assistant action, ≤ 4 named files, action bounded at
  200 chars) injected when the compaction gap exceeds the bound.
- Smoke: `scripts/smoke-checkpoint-rewind.py` (marker durable, rewind retains
  the report, exploration kept in the raw log, tools absent when off);
  `scripts/smoke-wait.py` (bounded timeout line; a recorded command failure
  later wakes wait from the attention rows).
- Tests: 637 unit tests + 13 doc + 91 integration green; clippy `-D warnings`
  clean. Notable fixes en route: wait parsed `seconds` as JSON number via
  `arg_raw` (arg_str only unquotes strings); tool schemas carried string
  `maxLength`/`maximum` literals the validator rejects — now numeric.
