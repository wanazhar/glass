# Review: reader-redraw-001

Status: **PASS — OFFLINE READER AND HISTORY GATES** (October 7, 2026)

## Findings

- The previous reader's first frame could retain stale glyphs where the new frame contained blank cells. I reproduced that on the old debug binary by writing distinct full-width sentinel rows directly to the PTY terminal output before Ctrl-B; the first reader frame left alternate sentinel rows visible. The fix clears the owned body before creating the reader terminal. The same clear-and-recreate path is used on resize and overlay restoration.
- History projection now omits reasoning deltas from the main transcript and presents them through the dedicated Ctrl-T view. Bash calls are associated with recorded Bash output by user turn and command occurrence, so repeated identical commands remain distinct. Unique tool call/results are combined into a readable entry; duplicate or ambiguous IDs remain visible instead of being silently dropped. Non-Bash error results and legacy Bash-only records remain present.
- Session JSONL serialization, model replay projection and `/details` source paths remain separate from the display-only history projection. The PTY fixture's session file SHA-256 was unchanged after reader and reasoning-view use.

## Verification

- `cargo build --offline --locked -p glass-core` passed. The same independent PTY script then passed against both the rebuilt debug binary and the exact private release install `/tmp/gc-final-install/bin/gc`.
- `/tmp/gc-reader-redraw-installed-review.log`: real PTY at 65×30, 40×24, 24×16, 20×12 and 20×8. Each case injects full-width stale sentinels into every body row through the PTY slave, opens the reader, and confirms all sentinel cells are gone while the composer draft and cursor remain intact. It also covers palette restore, scrolling, resize out and back, exactly one alternate screen, clean process exit, and restored mouse modes.
- The same PTY fixture contains fragmented reasoning, a Bash call/Bash echo/result sequence, two identical Bash executions, a reused `call_0` ID across turns and a compaction marker, a non-Bash call/result and failure, repeated identical legacy Bash-only records, and a standalone legacy Bash record. It verifies canonical visible output while paging the complete history, excludes reasoning from the main reader, checks both reasoning fragments through Ctrl-T, and confirms the exact JSONL digest is unchanged.
- Focused tests passed: 14 session tests (`/tmp/gc-reader-redraw-session-tests.log`) and 16 layout tests (`/tmp/gc-reader-redraw-layout-tests.log`). Strict scoped Clippy passed for `glass-core` and `glass-tui` (`/tmp/gc-reader-redraw-clippy.log`).
- The private release installation completed successfully (`/tmp/gc-reader-redraw-install.log`). The existing default `glass`/`glass-core` installation was not changed.

## Scope limit

This was an offline review using persisted synthetic session records; it made no provider request. Live provider streaming was not exercised.
