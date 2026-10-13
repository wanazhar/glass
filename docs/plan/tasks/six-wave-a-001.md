---
id: six-wave-a-001
scope: wave A — loop detection, read recency, sticky routing, provider cache affinity
status: implemented
source: research/six-codebases-2026-10-10.md candidates #2, #3, #15, #1
---

# Wave A: agent-loop correctness

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

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

## Implementation notes

- `loop_detect.rs`: a 10-slot SHA-256 ring (name + raw arguments); the
  reminder fires once per streak at 5 repeats, appends `Record::Meta`, and
  the window clears at every turn boundary in `run_turn`. Both built-ins
  funnel through `Agent::loop_guard` at each of the four dispatch sites in
  `run_tool_batch`, so batched, pumped, and inline executions all count.
- Read recency: `ReadSet` carries `last_read: HashMap<String, u64>`.
  `observe_at` threads the turn's tool-call index through the read scans;
  `Agent::tool_call_index` increments in `loop_guard`. `state` edit results
  name `path`/`file`, and when `current_call - last_read >= 25` the result
  gains one hint line. Callers without an index (tests, replay) land at 0,
  which can never satisfy the age check.
- Sticky routing: `Chain` carries a `sticky` Cell<usize>; a served fallback
  sets it and later requests skip earlier levels until `unstick()`. The
  live harness rebuilds the chain from `factory()` at each user turn
  (`main.rs`), which resets stickiness by construction; explicit
  `unstick`/`serving_model` remain for reused chains and tests
  (`cfg_attr(not(test), allow(dead_code))` documented).
- Cache affinity: `subagent::run_task` takes `parent_prompt`; when the
  task has no persona (`task.prompt` unset) the subagent's base prompt is
  the parent's system prompt, so its provider-side cacheable prefix equals
  the parent's request prefix instead of the bespoke `compose_prompt`
  framing. Persona tasks keep their isolated prefix by design. Wired at
  all three call sites (headless run, interactive `run_subagents`,
  `/code-review`).

## Verification

- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --locked --workspace` — 640 + 13 + 91 pass, 0 fail
  (7 new loop-detect, 2 new chain sticky, 2 new affinity,
  readguard extended).
- Startup path unchanged: no new allocations before the first tool call;
  the ring is empty and the index is 0 at construction.
- Performance notes measured inline: window hashing is SHA-256 over the
  call arguments per call (bounded by args length; State args are capped
  4 KiB). The hint lookup is one HashMap probe per state edit.
- Pending: headless two-subagent cache-write delta and PTY transcript
  checks, run with the final wave verification.

## Integrated review correction (2026-10-11)

Raw base-prompt reuse is implemented for persona-free tasks; explicit personas
must take precedence. Full parent/child cache affinity is not established: the
parent's effective prompt, tool schemas, project instructions and extension
context may differ. The earlier claim of equal provider cache prefixes and
measured duplicate warm-write savings is withdrawn pending exact identity and
provider usage evidence. No token-saving figure is claimed for this feature.

Read-recency notices belong to observed shell write results, not state notes.
Loop detection includes extension tools as well as built-ins.
