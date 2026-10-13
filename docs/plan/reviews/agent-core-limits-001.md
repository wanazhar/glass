# Model-managed teammate limits — 2026-10-10

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Changes

The main model receives compact delegation guidance and can explicitly set worker cumulative allowance, output allowance, reasoning hint and tool-round count. Create supports `--tokens`, `--output`, `--reasoning`, `--rounds`; `gc agent limits ID ...` changes per-turn limits without starting a model request. `budget` remains the explicit cumulative allowance control. Defaults are 100,000 cumulative tokens, 16,000 output tokens and 24 rounds. Reasoning remains provider-dependent; zero restores the provider default.

Receipts expose remaining allowance, reported and estimated usage, active reservations, last request input estimate, adapter context window, configured limits, typed failure category and recovery guidance. Lists expose mailbox counts rather than queued message text. The parent receives one low-allowance warning at 20% remaining and completion usage summaries, with no polling or automatic budget increases.

Context-allowance refusals compact deterministically once only when history can be freed. Output exhaustion returns to the parent for a deliberate limits/task decision. Completed calls retain their result IDs after both planned shrinking and overflow eviction, preventing false interrupted-call notices. Proactive compaction now sends its rebuilt projection on the first request rather than a stale pre-compaction snapshot. Remaining-budget output clamping cannot produce a request whose configured reasoning consumes the entire output allowance.

## Review

Root implemented; GPT-6-Luna medium reviewed validation, receipts, accounting and compaction. Invalid limits now fail before session/worktree creation. Reviewer follow-up confirmed retained call IDs preserve completed call/result pairing. No user processes were interrupted and no installed binary was replaced.

## Verification

- `cargo test --workspace`: 636 tests passed, zero failures; one ignored documentation test.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo build --release --package glass-core --bin glass-core`: passed. All following PTYs used `target/release/glass-core`.
- `scripts/smoke-agent-limits.py`: passed at 80×24, 40×12 and 20×8, including parent-controlled output recovery and near-budget compaction with no repeated read.
- `scripts/smoke-agents.py`: passed headless/non-Git accounting and 80×24, 40×12, 20×8 lifecycle, concurrency, restart, steering and privacy checks.
- `scripts/smoke-workflow-next.py`: all 22 cases passed, including 15 overlay PTYs, checkpoint/handoff/capture/fork and a five-request coding fixture (238 ms).
- `scripts/smoke-help.py`: aliases, focused guides, pipes, errors and color/no-color PTYs passed, including 20×8.
- `scripts/check-performance.py`: all configured budgets passed; measurements below.
- Added unit coverage for atomic limit validation, private mailbox summaries, retained refusal estimates, reasoning/output headroom, sanitized allowance recovery and immediately refreshed proactive context.
- Added `scripts/smoke-agent-limits.py` to CI. It uses an isolated local OpenAI-compatible fixture, real CLI/control sockets and PTYs at 80×24, 40×12 and 20×8. It reproduces reasoning-only output exhaustion, explicit parent reallocation/resume, near-budget context compaction, retained tool results, no implicit model calls for limits, and modal/composer responsiveness.

These tests exercise provider wire behavior deterministically. They do not promise that every model obeys a reasoning hint or that local token estimates equal provider billing. Cumulative allowance includes repeated and cached input; the main model must choose an appropriate allocation for the task.

## Final performance measurements

Median help startup: 1.06 ms. Worst screen-size median prompt startup: 6.50 ms; input visibility: 0.13 ms; redraw: 0.27 ms. Worst median 12,000-record history reader frame: 79.68 ms. These are local fixture measurements, not live provider latency.
