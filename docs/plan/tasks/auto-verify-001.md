---
id: auto-verify-001
scope: extensions/verify, extension host rewrite bridge
status: done
depends-on: []
---

## Objective

Aider's highest-measured guardrail, in glass-core's architecture: after a
bash command writes files, run the project's verifier and append failures to
the tool result. Ported from Aider's `--auto-lint`/`--test-cmd` and Claude
Code's verify-before-commit guidance (research/harness-delta-2026-10.md).

## Context

- **Core fix that made it possible**: `rewrite_tool_result` passed no
  host-tool bridge, so an extension issuing a nested `call_tool` during the
  `tool_result` hook got *silence* and timed out after 5s. Both rewrite call
  sites (bash funnel and general tool path) now build the same bridge
  `run_tool` uses (bash + state only, same guard), factored into a
  callback-shaped `with_tool_bridge` so the borrow of `state` cannot outlive
  the call. A hook without a bridge now receives an explicit refusal body
  instead of nothing.
- The verifier runs through the host's bash tool: sandbox policy, timeout,
  and output caps apply, and the extension needs no shell of its own.
- Debounce (default 8s) keeps a multi-write turn from running the verifier
  per command; `[verify ok]` fires only on the failure→success transition.

## Paths

- `crates/glass/src/ext/host.rs`, `ext/mod.rs`, `agent.rs` (bridge fix).
- `extensions/verify/verify.py` + README (new); `addons.rs` bundle entry.
- `scripts/smoke-verify.py` (end-to-end through the binary).

## Verification

- Selftest: exit parsing, capping, detection, debounce, back-to-green logic.
- `scripts/smoke-verify.py`: two writes in one turn; the first tool result
  carries `[verify exit 1]`, the second `[verify ok]`.
- `cargo test -p glass-core` (373 tests incl. the updated host tests) and
  clippy.
