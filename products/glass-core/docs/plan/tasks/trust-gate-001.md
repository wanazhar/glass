---
id: trust-gate-001
scope: crates/glass trust module, config, repl startup, headless run
status: done
depends-on: []
---
# Trust gate

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

A workspace trust gate for repository-controlled instruction text: clone a
repo and its `AGENTS.md` no longer flows silently into the system prompt.
Asked once per workspace, keyed by a content hash; changed instructions ask
again. Ported from pi's project-trust model and Claude Code's folder trust
(research/harness-features-2026.md), sized to what glass-core actually loads
from a workspace today (instruction files; extensions and skills are
user-level).

## Context

- Approvals live in `~/.config/glass-core/trust.json`, one entry per
  canonical workspace path holding `{hash, decision}`. A different hash makes
  the entry irrelevant, so edited instructions are a new decision.
- Interactive: a startup modal offers Allow / Deny / Not now. Headless runs
  cannot ask: a stored allow for the exact content, `trust = "always"`, or
  `--trust` accepts; anything else skips the instructions with an explicit
  stderr warning.
- Workflow turns that cannot prompt (`/plan`, `/init`) consult the store
  without asking, so they follow the same decision as the session.

## Paths

- `crates/glass/src/trust.rs` (new): digest (FNV-1a over path+text+truncation),
  store load/save (`auth::secure_write`, 0600), `remembered`/`remember`,
  `gated`/`gated_in`, tests.
- `crates/glass/src/config.rs`: `trust = "ask" | "always"`.
- `crates/glass/src/main.rs`: startup prompt in the repl, headless gating
  with `--trust` (also accepted by the run parser), warning text.
- `crates/glass/src/workflows.rs`: `/plan` and `/init` instruction loads go
  through `trust::gated`.
- `scripts/smoke-trust.py`; smoke-polish fixture pinned to `trust="always"`.

## Verification

- Unit tests: digest stability/content sensitivity, store round-trip, changed
  content re-decision, gated honoring config/force/store (hermetic store path).
- PTY: deny and allow flows, stored decisions skipping the prompt, content
  change re-prompting, headless warning without `--trust` and acceptance with.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
