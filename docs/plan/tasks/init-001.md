---
id: init-001
scope: crates/glass workflows init, /init command, gc init headless
status: done
depends-on: []
---
# Init

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

`/init [notes]` drafts an `AGENTS.md` for the workspace in a read-only run,
then offers save / review / regenerate; existing files require an explicit
overwrite confirmation. `gc init [--write] [--json]` does the same headlessly.
Ported from Claude Code `/init` and opencode's project-instruction generator
(research/harness-features-2026.md).

## Context

- Reuses the `/plan` read-only harness: plan-mode Agent with the `inspect`
  tool only — no shell, no writes, no extensions — so generating the file
  cannot modify the workspace before the user has seen it.
- The draft is written by the harness (not the model), only after the review
  modal chooses save; an outer Markdown fence, if the model wraps the whole
  document in one, is unwrapped first.

## Paths

- `crates/glass/src/workflows.rs`: `init_request`, `strip_fence`,
  `init_draft`, `init_modal`, `init`, `ephemeral_factory`, headless `init`.
- `crates/glass/src/main.rs`: `/init` dispatch, palette entry, help.
- `scripts/smoke-init.py`: PTY + headless suite.

## Verification

- Unit tests: request composition (notes, output-only instruction), fence
  unwrapping that leaves inner fences alone.
- PTY: fenced draft saved unwrapped; overwrite requires confirmation and
  cancel changes nothing; headless `--json` reports without writing and
  `--write` creates the file.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
