# Reliability and plugin focused review

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Status: focused review passed; full release gates pending integration.

Independent verifier reviewed root `1252259` reliability changes and plugin
worktree `f7ba6a4` (merged as `f4068df`).

## Reliability evidence

- 44 session tests, provider poison test and 33 extension tests passed.
- `scripts/smoke-reliability.py` passed recovery and durable serve dedupe checks.
- Actual LD_PRELOAD failure injection returned EIO after a partial ToolCall
  intent write. Rollback retained only the complete prior User record, bash did
  not run, and the mock provider received no second request.
- Actual read-only JSON export succeeded while a live serve held the writer lock.
- Canonical alias locks, external history changes, custom append acknowledgement
  and cache/log length synchronization passed focused coverage.

## Plugin evidence

- Six Rust plugin tests, integrity smoke and MCP smoke passed.
- Real MCP bridge launched a valid managed plugin. Tampered content and receipts
  with missing source/capability fields launched no server and exposed no tools.
- Rust and Python content hashes matched. Review found two bridge bypasses;
  both were fixed before approval.

## Remaining gates

Doctor and reader command integration must land before the frozen release build,
workspace tests, strict Clippy, complete smoke inventory and phone PTYs. This
report does not declare the twenty-item round complete.
