---
id: state-receipts-001
scope: roadmap item 42 — state duplicate-request detection, durable receipts and explicit ownership
status: ready
depends-on: []
---

# State receipts, duplicate-request detection and ownership

Roadmap item 42. The state tool has an optimistic concurrency guard but no
idempotency. A retried mutation currently either overwrites or fails with a
stale revision; neither outcome is safe to replay automatically. This task adds
the missing duplicate-request handling and makes the durable receipt describe
what was actually applied.

## Objective

1. **Duplicate-request detection.** The `state` tool has no idempotency key and
   no request ledger, so a retried write can apply twice. Accept an explicit
   request id, record it durably with the result, and make a repeat of a known id
   return that recorded receipt instead of re-applying. An id that is reused
   with *different* content is a client error and must fail loudly, not be
   treated as a replay.
2. **Durable receipts.** Durability today is a full-replacement
   `Record::State` with no op id, actor, timestamp or acknowledgement. Extend the
   record so a replay shows what was applied, by whom, and under which id.
   Preserve backward compatibility: existing logs without the new fields must
   still restore.
3. **Explicit ownership.** The revision read-back hardcodes the string
   `"current session; durable log writer lock"`. Compute real ownership from the
   session and the log writer, and state the revision the caller must present.
   Ownership must reflect who can actually write, not an assertion.

## Context

- [Tooling foundation](tooling-foundation-001.md) — item 42 scope: consistent
  validated operations, durable update receipts, explicit ownership, and
  duplicate-request detection where a retry could repeat a mutation.
- [Durability](durability-001.md) — the existing log and effect-sandwich model
  this must build on rather than duplicate.
- `docs/INDEX.md` — document index.

## Path

Own these paths only.

- `crates/glass/src/tools.rs`
- `crates/glass/src/session.rs`
- `crates/glass/src/provider/mod.rs`
- new `crates/glass/src/state_receipts.rs` if a separate module is cleaner
- `scripts/smoke-*.py` for the new state receipt cases
- `docs/plan/tasks/state-receipts-001.md`

Do not edit `crates/glass/src/main.rs`, `crates/glass/src/ext/**`,
`crates/glass/src/addons.rs` or `crates/glass/src/attention.rs`; other tasks
own those in this wave.

## Constraints

- A receipt must describe what was durably accepted or applied. Never write a
  receipt for work that was not applied, and never acknowledge before the log
  append succeeds. A failed durable append must roll back, not leave a receipt.
- Preserve the existing bounds: 128 items, 4 KiB per item, 64 KiB total.
- Preserve the existing revision guard. Adding idempotency must not weaken it.
- Old session logs without receipts must restore unchanged. This is a hard
  compatibility requirement, not a preference.
- Replayed or duplicated requests must not duplicate the user-visible record.
- Bounded: the request-id ledger must not grow without limit. State the bound
  and the eviction rule in the output rather than silently dropping ids.

## Verification

- Unit tests for: replay of a known request id returns the recorded receipt
  without re-applying; the same id with different content fails explicitly; a
  concurrent update still trips the stale-revision guard; a failed log append
  rolls back and writes no receipt; restoring an old receipt-free log succeeds;
  the ledger bound is enforced and reported.
- A regression check that a retried `state` write cannot double-apply.
- `cargo clippy --workspace --all-targets -- -D warnings` green.
- `cargo test --locked --workspace` green.
