---
id: six-wave-a-001
scope: wave A — loop detection, read recency, sticky routing, provider cache affinity
status: ready
source: research/six-codebases-2026-10-10.md candidates #2, #3, #15, #1
---

# Wave A: agent-loop correctness

Four small changes in the turn loop, implemented as one task because they
touch the same files and the same tests. They are independent features with
independent acceptances.

## Objective

1. **Loop detection** (crush `loop_detection.go`): a SHA-256 ring window over
   the last 10 tool calls (name + canonicalized arguments); ≥5 identical
   hashes in the window emits one `Progress::ProviderNotice` and appends one
   `Record::Meta` reminder naming the repeated call and advising a different
   approach, then resets the window. Never cancels, never fabricates a
   transcript event — the model decides. An identical *batch* of different
   calls does not count; only single repeated (name, args) pairs do.
2. **Read recency** (crush `filetracker`): extend `ReadSet` to record the
   tool-call index at which each file was last read. When `state`'s edit
   validation issues a stale/ambiguity hint, or the model edits a file last
   read more than `N` tool calls ago (N=25), the result gains one line:
   "last read K tool calls ago; reread before further edits". No new tool,
   no new prompt surface.
3. **Sticky routing** (pi virtual models): when a mid-turn model switch is
   recorded (GC already logs the answering model per request), tool-result
   follow-up requests within the same turn keep the answering model until
   the next user message, instead of immediately using the newly selected
   model. A user message takes effect immediately. Record which model
   answered each turn so resume reproduces the same stickiness.
4. **Provider cache affinity** (opencode `affinity.ts`): nested subagent
   sessions whose system prefix equals the parent's share the parent's
   cache-warmer prefix bytes. Concretely: `subagent::run_task` passes the
   parent's current `prefix_bytes` and base prompt so the nested warm
   decision reuses the parent's warm write instead of issuing its own
   duplicate cache write. Measured as a drop in duplicate cache-write
   tokens on a two-subagent batch.

## Path

Own these paths only.

- `crates/glass/src/agent.rs` — loop window, sticky routing fields
- `crates/glass/src/readguard.rs` — read recency
- `crates/glass/src/subagent.rs` — affinity handoff fields
- `crates/glass/src/warmer.rs` — prefix reuse
- `crates/glass/src/tools.rs` — hint text in state results
- `docs/plan/tasks/six-wave-a-001.md`

Do not edit `provider/mod.rs` request struct shape, `session.rs` Record
enum, or `ext/**`; other waves own those. Persistence needs no new record
type: recency and stickiness derive from existing records.

## Constraints

- Startup path untouched: no new work before the first turn begins.
- Loop window is bounded (10 entries) and cleared at turn boundaries.
- No new dependencies; hashing uses SHA-256 already in the dependency tree
  (`ring`) or a std hasher — document the choice.
- Sticky routing must not fight model fallback: fallback failure still
  fails over; stickiness only applies to explicit selection.

## Verification

- Unit tests: window fires at exactly 5 repeats, clears after reminder,
  survives steer; recency hint appears only past the threshold; sticky
  routing persists across tool rounds and resets on user input; the
  affinity handoff reuses parent prefix identity.
- Headless JSON check: two-subagent batch reports cache-write totals
  smaller than before the change (record the numbers).
- `cargo clippy --workspace --all-targets -- -D warnings` green.
- `cargo test --locked --workspace` green.
- PTY: a scripted mock provider that repeats one bash call six times shows
  one reminder in the transcript; performance gate: time-to-first-token
  and startup within release budget.
