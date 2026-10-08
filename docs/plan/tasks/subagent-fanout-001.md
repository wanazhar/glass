---
id: subagent-fanout-001
scope: crates/glass extension host, subagent batch runner, extensions/subagent
status: done
depends-on: []
---

## Objective

Upgrade delegation from one synchronous sub-agent to a batch: one to eight
self-contained tasks per call, up to four executed concurrently, reports
returned in request order, optional named agent definitions and isolated
git-worktree execution. Ported from the agent-team/subagent fan-out that
Claude Code, Codex, Copilot `/fleet`, opencode, Roo, Goose, and Kilo shipped
(see research/harness-features-2026.md).

## Context

- The extension host is one serial stdio pipe per extension
  (`Extension::request` is `&mut self`), so fan-out cannot come from
  overlapping tool calls; the host must run the nested loops inside a single
  request.
- A latent bug was fixed on the way: the reply deadline was not extended
  while a nested subagent or tool chain ran, so any delegated task longer
  than the 50s wait expired as "unavailable".
- Also fixed a stale painter test that asserted on colored output as if it
  were plain text (it could not pass once code runs got theme colors).

## Paths

- `crates/glass/src/ext/protocol.rs`: `SubagentTask`, `parse_subagent_tasks`,
  `is_subagents_call`, `subagents_result_json` (additive; the legacy
  `run_subagent` single-task form still works).
- `crates/glass/src/ext/host.rs`: `SubagentFn` takes a task batch; the demux
  services `run_subagents`, answers malformed batches with an error, and
  resets the wait budget after nested work.
- `crates/glass/src/subagent.rs` (new): `run_batch` wave scheduler (cap 4),
  `run_task` nested-agent construction, `worktree` isolation, prompt
  composition.
- `crates/glass/src/main.rs`: per-model provider factory (`make_provider`,
  `ProviderForModel`), both runner call sites (interactive + headless).
- `extensions/subagent/subagent.py`: batch schema, agent-definition loading,
  `agents` command, report formatting.

## Integration

Extension `run_tool` → `run_subagents` nested request → host `SubagentFn` →
`run_batch` (scoped threads) → `run_task` per task (fresh provider, session,
state) → reports in call order → formatted by the extension.

## Verification

- `cargo test -p glass-core`: batch ordering, concurrency cap, panic
  containment, prompt composition, worktree creation/failure, protocol
  parsing, malformed-batch error path, end-to-end extension round-trip.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- Manual: `/agents`, a two-task batch with mocks, `worktree: true` leaving an
  inspectable checkout.
