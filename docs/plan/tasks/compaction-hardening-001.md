---
id: compaction-hardening-001
scope: crates/glass compact module, agent loop, /compact command
status: done
depends-on: []
---
# Compaction hardening

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

Harden context management with three ports from the harness survey
(research/harness-features-2026.md, research/harness-context-management-2026.md):

1. A manual `/compact [focus]` that force-summarizes below the threshold.
2. Overflow-refusal recovery: a provider "prompt too long" refusal triggers
   deterministic truncation and one retry — never a re-stream of the history
   that just did not fit (the Crush #3620 repeated context-overflow failure).
3. A circuit breaker: three consecutive summary failures stop paid
   summarization for the session while deterministic compaction continues
   (instead of qwen-code's original one-failure permanent lock).
4. Bounded summary serialization: each record is capped at 2,000 characters
   before it reaches the summarizer, so the summary request fits the window
   it is replacing (pi's serialize discipline).

## Paths

- `crates/glass/src/compact.rs`: `summary_prompt_bounded` + `SUMMARY_RECORD_CAP`,
  `manual_prompt`, `run_manual`, `evict_oversized`, `looks_like_overflow`,
  `Breaker` + `SUMMARY_FAILURE_LIMIT`, tests.
- `crates/glass/src/agent.rs`: `breaker`/`overflow_recovered` fields,
  `force_compact`, `note_summary_failure`, run_turn recovery wrapper,
  an accurate partial-response Meta record (only when something survived), tests.
- `crates/glass/src/main.rs`: `/compact` dispatch, palette catalogue, help.
- `README.md`: context-management section.

## Integration

`/compact` → `compact::run_manual` (bare provider request, no tools) →
`apply_summary` + `Record::Compacted{summary}` so resume rebuilds the same
projection. Provider refusal → `run_turn` guard → `force_compact`
(evict oversized anywhere + rebuild middle) → one retry. Summary failures →
`Breaker`; the open state skips only the paid path.

## Verification

- Unit tests: record capping and truncation marker, eviction (including the
  newest/protected result), overflow-message matrix vs. other errors,
  breaker transitions, focus prompt, recovery retry-once, no-recovery
  without freeable context, non-overflow failures untouched.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- Manual: `/compact` with and without focus on a long session; resume shows
  the same projection; overflow recovery on a mock refusing endpoint.
