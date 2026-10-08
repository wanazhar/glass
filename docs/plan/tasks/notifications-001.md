---
id: notifications-001
scope: crates/glass notify module, config, interactive turn end
status: done
depends-on: []
---

## Objective

Get attention when a long turn finishes or any turn fails: OSC 9 desktop
notifications on terminals that support them, a terminal bell everywhere
else, `notify = "auto" | "osc" | "bell" | "off"` in config. Ported from
Crush (`auto|native|osc|bell|disabled`), Codex notifications, and Amp's
"ready when you are" alerts (research/harness-features-2026.md).

## Context

- The notification is the cheapest feature that makes unattended runs
  practical; everything else (fan-out, background jobs, budgets) is worth
  less if you cannot walk away.
- tmux is deliberately not treated as OSC-capable: OSC 9 is swallowed
  without passthrough, so the bell (window flag) is the honest default.
- Provider or tool text must never reach the escape sequence; messages are
  composed by the harness and sanitized anyway.

## Paths

- `crates/glass/src/notify.rs` (new): `Mode`, `wanted`, `sequence`, `emit`,
  terminal capability list, message sanitization.
- `crates/glass/src/config.rs`: `notify` field, strict parse, tests.
- `crates/glass/src/main.rs`: turn timing, failure-always and
  finished-and-slow gates, follow-up deferral.

## Integration

Turn result handling in the REPL → `notify::wanted` policy → `notify::emit`
(OSC 9 or BEL to stdout, best effort). Headless runs never call it.

## Verification

- Unit tests: mode parsing, sequence selection per terminal, sanitization
  against control-character smuggling, policy matrix (fast/slow ×
  finished/failed/cancelled).
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- Manual: long turn and failed turn on an OSC-capable terminal; bell on a
  dumb terminal; queued follow-up defers the alert.
