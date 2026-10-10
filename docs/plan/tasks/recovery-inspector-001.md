---
id: recovery-inspector-001
scope: roadmap item 24 — failure recovery inspector with failure class, attempts and safe actions
status: done
depends-on: []
---

## Delivered

1. **Attempt lineage.** `crates/glass/src/attempt_log.rs` appends one
   `<session>.attempts.jsonl` line per retry from the existing
   `Progress::Retrying` emission point in `agent.rs` — attempt number, the
   backoff actually applied, the caller-safe cause, the session record index
   the retry belongs to, a write-time clock reading, and the provider request
   id when the provider returned one. The aggregate retry count is no longer
   the only trace.
2. **Safe next action.** `crates/glass/src/recovery.rs` classifies each
   recorded failure and attaches exactly one bounded action: retry, fix the
   input, check authentication, raise the context window, or wait.
   `Recommendation::replays_completed_mutation()` is a method rather than a
   field, so no recommendation can claim a replay; when the log shows a
   completed tool result in the failed turn the detail says so explicitly.
   Unrecognised causes map to *wait* — nothing is asserted about a cause the
   classifier did not match.
3. **Transport reset.** `crates/glass/src/transport/` owns
   `reset_safety`/`reset`: the provider is rebuilt from the current
   configuration and exactly one `recovery/transport-reset` marker is appended
   to the session log. The inspector offers it only for a transport-class
   failure and only when no turn is streaming.
4. **Navigable inspector.** `/recovery` now opens a selectable list
   (severity · class · attempts · timestamp · action) whose detail view keeps
   the formatted and raw JSON renderings, and whose list carries the
   shareable diagnostic bundle and the local JSON export. `/diagnostics`
   remains the shareable bundle; `gc diagnostics SESSION --inspector --json` is
   the headless local view.
5. **Provider request identifiers.** `transport::request_id` reads the id the
   provider actually returned in its own text, and returns nothing when there
   is none — never a synthesised one, never an endpoint. The inspector shows
   it; `diagnostic_bundle` still omits it, and `--inspector` refuses `--save`.

### Notes and limitations

- Provider adapters were not in this task's path set, so a provider identity is
  captured from the diagnostic text an adapter already returns
  (`response=<id>` and `request-id`) rather than from a new event field. The
  placeholders adapters substitute when a stream carries no id (`not
  reported`) are rejected, so an absent id stays absent.
- The runtime already rebuilds its provider per turn, so a transport reset has
  no pooled connection to drop. What it guarantees is credential
  re-resolution and endpoint re-validation at the moment the user asks, plus a
  durable marker; `transport::reset` documents this rather than claiming a
  teardown that does not happen.
- `diagnostic_bundle`'s classifier gained HTTP status rules. A 403 or a 503
  previously matched none of the text rules and were reported as
  `none_or_unspecified`, which would have recommended a retry for a credential
  failure.

# Failure recovery inspector

Roadmap item 24. `/recovery` and `gc diagnostics` render a raw JSON snapshot in
a modal. The failure classification exists; the attempt lineage, cooldown and a
safe next action do not. This task turns the snapshot into an inspector a user
can act on, without inventing data the runtime does not have.

## Objective

1. **Attempt lineage.** `provider::stream_with_retry` keeps `attempt` as a local
   variable and never persists it; `RETRY_ATTEMPTS`/`RETRY_DELAYS` are fixed
   constants, not cooldown state. Record each retry attempt durably with its
   attempt number, delay, and cause so an inspector can show what actually
   happened rather than a single aggregate retry count.
2. **Safe next action.** No `next_action` concept exists anywhere. Classify each
   recent failure and attach one bounded, safe recommendation: retry, fix the
   input, check authentication, raise the context window, or wait. The action
   must never imply that a completed mutation will be replayed.
3. **Transport reset that preserves local history.** Add a reset primitive that
   re-establishes the provider transport without discarding the local session
   log, and make the inspector offer it only when a reset is actually safe.
4. **Navigable inspector.** Replace the raw JSON modal with a selectable failure
   list: severity, class, attempts, timestamp, and the recommended action. Keep
   the raw JSON view available for export and debugging.
5. **Provider request identifiers where available.** `diagnostic_bundle` lists
   response IDs under *redaction*, so they are dropped. Capture the provider
   request/response id when the provider returns one, keep it in the inspector,
   and keep it out of any shareable bundle.

## Context

- Roadmap item 24 in
  [research/t3code-parity-2026-10-10.md](../../../research/t3code-parity-2026-10-10.md).
- [Tooling foundation](tooling-foundation-001.md) — shared structured error and
  artifact contracts; the inspector must reuse them.
- [Model fallback](model-fallback-001.md) — the existing failover chain the
  recommendation must stay consistent with.
- `docs/INDEX.md` — document index.

## Path

Own these paths only. They are disjoint from the other tasks in this wave.

- `crates/glass/src/agent.rs` — the retry closure at `agent.rs:629` is where
  each attempt is recorded durably; it already receives attempt, delay and cause.
- `crates/glass/src/main.rs` — only to upgrade the existing `/recovery` entry
  point from a raw JSON modal to the navigable inspector, and to register any
  new command. Keep every unrelated command untouched.
- `crates/glass/src/diagnostic_bundle.rs`
- `crates/glass/src/attention.rs`
- `crates/glass/src/transport/`
- new `crates/glass/src/recovery.rs` — inspector and safe-action classification
- new `crates/glass/src/attempt_log.rs` — durable attempt records
- new `scripts/smoke-recovery.py`
- `docs/plan/tasks/recovery-inspector-001.md`

Do not edit `crates/glass/src/provider/mod.rs`,
`crates/glass/src/tools.rs`, `crates/glass/src/session.rs`,
`crates/glass/src/ext/**`, `crates/glass/src/addons.rs` or
`crates/glass/src/workspace.rs`; other tasks own those in this wave.
`/recovery` is already wired to `diagnostic_bundle::snapshot`, so keep that
entry point working.

Note: the `state` tool schema in `provider/mod.rs` belongs to another task.
Record attempts through the existing `Progress::Retrying` emission instead of
adding a new provider-facing tool schema.

## Constraints

- Never present an estimate as a measurement. If an attempt count, cooldown or
  request id is not known, say it is unknown. No invented request ids.
- Never silently replay a completed mutation. The safe next action must
  distinguish "safe to retry the request" from "the mutation already happened".
- The shareable diagnostic bundle keeps its current redaction allowlist. The
  inspector may show a provider request id locally; the bundle must still omit
  it, along with prompts, paths, credentials and endpoints.
- No new background service and no added startup work. The inspector reads
  records that already exist on disk.
- Bound the failure list and state the bound. Reading the inspector must not
  load full transcript bodies.

## Verification

- Unit tests for: attempt records are written per retry with attempt number and
  cause; a known failure class maps to exactly one safe action; an unknown
  provider id is reported as unknown rather than fabricated; a transport reset
  preserves the session log; the shared bundle still omits request ids while
  the local inspector shows them; a completed mutation is never marked
  safe-to-replay.
- `cargo clippy --workspace --all-targets -- -D warnings` green.
- `cargo test --locked --workspace` green.
- A headless JSON check that the inspector output contains no prompt text,
  credentials or endpoint URLs.

## Verification

- Unit tests: every retry writes a record carrying its attempt number, delay
  and cause; a known class maps to exactly one action and the classifier's
  range is covered; a retry is bound to the failure that followed it; an
  unrecorded clock and an absent provider id both stay unknown; a completed
  mutation is measured from the log and never marked safe to replay; a
  transport reset preserves every prior record and appends one marker; the
  bundle omits prompt text, paths, output, credentials, endpoints and the
  provider id while the local inspector keeps the id; the failure list is
  bounded and holds no transcript body.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --locked --workspace` — 585 + 13 + 91 pass, 0 fail.
- `scripts/smoke-recovery.py` — both phases pass. The new phase drives a mock
  endpoint through a retried 429, a persistent 503 and a 403, and asserts the
  headless inspector JSON carries no prompt text, credential or endpoint URL,
  that the shareable bundle omits the provider id, and that `--inspector`
  refuses `--save`.
- PTY check: `/recovery` lists the failure rows, opens the detail with the
  lineage and the recommendation, and the reset entry reports the record count
  going 3 → 4 with the log prefix untouched.
