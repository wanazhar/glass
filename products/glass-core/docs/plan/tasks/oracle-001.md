---
id: oracle-001
scope: extensions/oracle, SubagentTask read_only
status: done
depends-on: [subagent-fanout-001]
---
# Oracle

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

`/oracle [question]`, the second-opinion pattern (Amp `oracle`, Claude
`/advisor`): review the current diff with a pinned alternate model, inline
and unrecorded, from a reviewer that cannot write.

## Context

- `SubagentTask` gains `read_only` (additive JSON field); `run_task` builds
  the nested agent with `with_plan_mode(true)`, so the reviewer gets the
  `inspect` tool only — no bash, no writes, no extensions. `compose_prompt`
  words the tooling accordingly.
- The extension gathers `git diff HEAD` itself (stat header + a note when
  untracked files exist, 60 KB cap) — a command hook has no host-tool
  bridge, and the oracle has no business running arbitrary shell through
  one anyway.
- Model comes from `~/.config/glass-core/oracle.json` or `GC_ORACLE_MODEL`;
  a missing model is a clear message, not a guess. Output prints through
  the command path (ui_println), not the transcript.

## Paths

- `crates/glass/src/ext/protocol.rs` (field + parse + test),
  `crates/glass/src/subagent.rs` (plan mode + prompt wording).
- `extensions/oracle/oracle.py` + README; `addons.rs` bundle entry.
- `scripts/smoke-oracle.py`.

## Verification

- Selftest: diff cap, missing-model message, no-diff message.
- Protocol unit test for `read_only` parsing.
- `scripts/smoke-oracle.py`: PTY `/oracle any risks?` in a dirty repo; the
  mock asserts the request used the configured model, carried the diff, and
  offered `inspect` with no `bash` tool; the critique renders inline.
- `cargo test --workspace` (383 at the time) + clippy.
