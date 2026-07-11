---
id: perf-001
scope: browser data plane
status: done
depends-on: []
---

# Compact observation and CDP hot path

## Objective

Make default observation compact and avoid full-DOM/event work on the hot agent path.

## Context

- `docs/architecture/README.md`
- `docs/architecture/browser.md`
- `docs/plan/analysis/performance-overhaul.md`

## Path

- `src/browser/cdp.rs`
- `src/browser/session.rs`
- `src/browser/dom.rs`
- `src/cli/args.rs`
- `src/cli/runner.rs`
- `tests/`
- `docs/architecture/browser.md`

## Requirements

- `observe()` must not fetch or cache a deep DOM or screenshot.
- Add explicitly named deep-DOM observation path/options without breaking explicit screenshot behavior.
- Selector lookup must fetch only the document root.
- Remove unused default CDP domain/event payload work without changing required invalidation behavior.
- Add deterministic tests for compact context, explicit deep DOM, and root-only selector lookup.

## Verification

- Focused browser/session and CDP tests.
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`

## Commit

`perf: slim default browser observation`

## Completion

- Default observations cache only compact page state; deep DOM and screenshots are explicit additions.
- Visible text is bounded to a UTF-8-safe 16 KiB budget before it becomes agent context.
- The byte cap is applied after complete page text is parsed, preserving supplementary Unicode characters at the former UTF-16 boundary.
- Cached accessibility context is bounded to 128 nodes, 32 interactive controls, and 4 KiB of UTF-8-safe AX label text; full snapshots remain explicit.
- Selector lookup uses a root-only DOM request, and default CDP events carry no payload data.
- Verification passed: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-targets`.
