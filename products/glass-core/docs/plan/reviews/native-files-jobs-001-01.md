# Native files and owned jobs verification

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Date: 2026-10-09. Root implementation, two GPT-6 Luna medium reviewers; final reviews found no blockers.

## Delivered

- Bounded numbered reads with whole-file SHA-256 revisions; batch range/exact edits and create-only or revision-checked atomic writes.
- Symlink/special-file/hardlink protections, permission preservation, CRLF/BOM/EOF handling, bounded colored previews preserving source indentation.
- Structured JSON, Python and JavaScript syntax diagnostics. External checkers are explicit, time bounded and isolated from project startup hooks.
- Session-owned background start, incremental poll, stdin and graceful stop through the existing bash tool; shared UI registry, bounded logs, process-group cleanup.
- Exactly two default tools, no new dependencies. Native CLI works headless and outside Git without provider configuration.

## Evidence

- `cargo test --workspace`: 601 passed, zero failed, one ignored doctest.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- Debug and release `scripts/smoke-native-files-jobs.py`: passed real PTYs at 80x30, 40x24, 20x8, plus headless/non-Git checks, single JSON error output, exact session records, schema count and job cursor checks.
- Existing feature, chat, live, and inline-scroll PTY suites passed, including 20x8 and background cleanup on exit.
- Release `scripts/check-performance.py`: prompt startup worst median 5.26 ms, key visibility 0.183 ms, redraw 0.297 ms; 12,000-record reader frame 80.31 ms, reader peak RSS 3492 KiB. All budgets passed.
- Release `scripts/check-native-performance.py`: approximately 100 KB / 1 MB / 8 MB reads took 1.48 / 3.25 / 18.06 ms median; revision-checked 1 MB edits 25.22 ms. Ordinary Python writes launched no syntax checker. All budgets passed.
- Compact OpenAI schema envelope: 1151 to 1698 bytes (+547). Additional prompt guidance approximately 45 tokens by rough byte estimate. Help is loaded on demand.

## Limits

Timing is local host measurement, not a real-model comparison with Pi. Existing-file replacement rechecks revisions but cannot prevent an uncooperative writer racing the final POSIX rename. Syntax diagnostics cover supported parsers, not project type checking. Jobs use pipes rather than terminal emulation, retain at most 4 MiB of logs, and own process groups rather than deliberately escaped descendants. Switching sessions retains jobs until stopped or application exit.
