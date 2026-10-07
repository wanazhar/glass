---
id: issue60-p1-debug-variable-status-001
scope: glass-dev/tui-debug-variable-preview-status
status: complete
depends-on: [issue60-p1-worker-request-coalescing-001]
---

# Issue #60 P1 F20: resolve debugger variable preview status

## Objective

Resolve F20 by ensuring stale Debug scopes and variables previews cannot leave
the TUI showing a `Loading` or `Queued` message forever. Report scopes that
have no expandable variables, and reject invalid references without submitting
a worker request.

## Context

- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/render.rs`
- `docs/architecture/development-tui.md`
- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), finding F20
- [Source report #45](https://github.com/wanazhar/glass/issues/45), section 20

## Contract

- Debug scope and variable preview status belongs to the captured request.
  When the selected session, frame, or scope becomes invalid, a matching
  `Loading …` or `Queued …` status resolves to a terminal stale-preview state.
  A different status string belongs to a newer user action and is preserved.
- A stale owned result is ignored and cannot restore old scopes or variables.
  Snapshot removal of the selected debugger session also clears its dependent
  thread, frame, scope, and variable projection before reconciling preview
  ownership.
- `variablesReference == 0` produces a visible no-expandable-variables state;
  a negative reference produces an invalid-reference state. Neither submits a
  variables request.
- A Code source-jump preview may update debugger data, but it does not replace
  Code's status. Invalid-reference and stale-preview handling preserve that
  status as well.

## Path

- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/render.rs`
- `docs/architecture/development-tui.md`
- `docs/plan/README.md`
- `docs/plan/analysis/issue-60.md`
- `docs/plan/tasks/issue60-p1-debug-variable-status-001.md`

## Verification

- Add focused coverage for stale active scope and variable results after a
  snapshot removes their session/frame, stale pending variable previews,
  zero/negative references, and Code status preservation.
- Run the focused TUI tests, `cargo check -p glass-dev --lib --bins --locked`,
  formatting, diff checks, and the relevant documentation gates.
- Record unrelated documentation-coverage drift separately from F20 evidence.

## Validation evidence

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test --manifest-path Cargo.toml -p glass-dev --lib --locked debug_` — passed (13 tests; 0 failed). This includes active stale scope and variable results after snapshot session removal, a queued stale variable drop, zero/negative references, Code status preservation, and the existing Code source-jump and debugger-pane ownership regressions.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check --manifest-path Cargo.toml -p glass-dev --lib --bins --locked` — passed. It reported 72 existing `glass-browser` dead-code warnings; no new `glass-dev` warnings.
- `cargo fmt --manifest-path Cargo.toml --all -- --check` and `git diff --check` — passed.
- `python3 scripts/check-documentation-depth.py` — passed; 93 current guides routed/audited and 19 substantive contracts.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-issue60-f20-release-documentation.json` — passed; 1,561 Markdown files scanned and 0 current-claim failures.
- `python3 scripts/check-tui-shortcuts.py` — passed; 15 implementation keys and 63 documentation markers.
- `python3 scripts/check-documentation-coverage.py --glass /home/ubuntu/work/glass/target/debug/glass --glass-browser /home/ubuntu/work/glass/target/debug/glass-browser` — the known unrelated MCP inventory drift remains: live tools differ from `crates/glass-dev/tests/fixtures/client-conformance-v1.json`, and `docs/mcp-schema-budget.md` lacks the measured `| Negotiated tools | 177 |` and `| Serialized `tools` array | 76,967 UTF-8 bytes |` entries. F20 does not change MCP tools or their schema.
- Independent [review 01](../reviews/issue60-p1-debug-variable-status-001-01.md) passed at implementation commit `7a647c71` with no blocking findings.
