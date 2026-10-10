---
id: recovery-inspector-001
scope: roadmap item 24 — failure recovery inspector with failure class, attempts and safe actions
status: ready
depends-on: []
---

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

Own these paths only.

- `crates/glass/src/diagnostic_bundle.rs`
- `crates/glass/src/attention.rs`
- new `crates/glass/src/recovery.rs`
- new `crates/glass/src/provider/recovery.rs` if the attempt record belongs there
- `crates/glass/src/provider/mod.rs`
- `crates/glass/src/transport/`
- `docs/plan/tasks/recovery-inspector-001.md`

Do not edit `crates/glass/src/main.rs`, `crates/glass/src/tools.rs`,
`crates/glass/src/session.rs` or `crates/glass/src/ext/**`; other tasks own
those in this wave. `/recovery` is already wired to
`diagnostic_bundle::snapshot`, so keep that entry point working.

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
