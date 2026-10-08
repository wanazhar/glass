---
id: export-001
scope: crates/glass export module, /export command, gc export
status: done
depends-on: []
---

## Objective

Export a session as one Markdown document: `/export [path]` interactively and
`gc export <id|--latest> [--out PATH] [--thinking] [--json]` headlessly.
Reported as a gap in the harness survey (opencode/Claude Code exports), and
`session.rs` already noted the log exists "so a future export has a source".

## Context

- The raw log is the source, not the compaction projection: an export is the
  record of what happened, including turns compaction dropped from the model's
  view.
- Tool output folds into `<details>` blocks whose fence is one backtick longer
  than the longest run in the content, so embedded fences cannot break the
  document.
- Reasoning is private working thought and is excluded unless `--thinking`.

## Paths

- `crates/glass/src/export.rs` (new): `markdown`, `raw_records`, fence
  escalation, tests.
- `crates/glass/src/main.rs`: `headless_export`, `/export` dispatch, palette,
  help; `format_time` made `pub(crate)` for the header.
- `scripts/smoke-polish.py`: headless export checks (stdout, --out, --json).

## Verification

- Unit tests: headings and folded output, exit statuses, state and meta,
  reasoning included only on request, compaction markers skipped, fence
  escalation, raw-log fallback.
- Headless: `export <id>` prints, `--out` writes, `--json` reports metadata.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
