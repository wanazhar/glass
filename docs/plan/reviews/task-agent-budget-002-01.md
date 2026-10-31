# Independent review: shared Core budget admission

Status: **pass for this bounded foundation**. This review covers commit
`cb01c740` on the isolated `task-agent-budget-002` branch. It does not certify
the complete task-backed coordination design or an installed/runtime rollout.

## Findings

- **P2, non-blocking — the full root envelope remains unimplemented.** The
  shared ledger enforces configured dollar admission and a 128-child lifetime
  cap when a cost limit is enabled. It does not add cumulative provider-request,
  model-turn, elapsed-time, retry, event, or owner-wake counters. A future
  coordination adapter must reserve those limits separately and must not cite
  this slice as proof that the entire root envelope is enforced.
No blocking findings remain for shared dollar admission in this task.

## Review evidence

- `budget::Context` owns one `Arc<Mutex<Ledger>>` and persistence path. The
  runtime's `run()` builds both parent and model-specific child factories via
  the same `runtime_provider_factories` helper. The helper-level concurrency
  regression asserts that exactly one of two simultaneous requests reaches a
  fake provider when only one reservation fits.
- Request cost is reserved and persisted before provider dispatch. Missing or
  partial usage, missing priced cache buckets, transport uncertainty, restart
  with an outstanding reservation, persistence failure, and reported cost
  above the reserved bound all fail closed. Known actual charges remain
  recorded; an over-bound report also marks the ledger unknown.
- Reservation IDs are monotonic and settlement/abandon operations are
  idempotent. Persisted in-flight reservations become unknown after reload.
  Provider-specific models in a child use their own rate card through the
  shared ledger.
- Child IDs are counted toward the 128 lifetime cap only when a cost limit is
  configured. Standalone sessions without that opt-in retain existing child
  behavior.
- Usage accounting preserves per-bucket presence. OpenAI prompt/output
  counters and Anthropic input/output counters are required; omitted cache
  buckets remain unknown when their rates are nonzero.

## Validation reported for the frozen commit

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-core --bin glass-core budget::tests --locked` — 16 passed.
- `cargo test -p glass-core --bin glass-core parent_child_factories_share_one_budget_scope_under_parallel_dispatch --locked -- --nocapture` — 1 passed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-core --bin glass-core provider::openai::protocol_regressions --locked -- --nocapture` — 14 passed. This parser filter ran before the final budget-only derive/settlement edits; parser source was unchanged afterward.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-core --bin glass-core provider::anthropic::tests --locked -- --nocapture` — 12 passed. This parser filter ran before the final budget-only derive/settlement edits; parser source was unchanged afterward.
- `cargo clippy -p glass-core --bin glass-core --tests --locked -- -D warnings`
  — passed.
- `cargo fmt --all` applied formatting and `git diff --check` passed on the
  final source. A final `cargo fmt --check` was not reported.

No provider account, install, service restart, or push was used for this
review.
