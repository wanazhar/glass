# Streaming and scrolling presentation

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

2026-10-10. User screenshots showed lost streaming colors, a selected-tool ID
footer in ordinary scrolling, and missing usage summaries in transcript replay.

## Fixes

`surface::emit` scopes terminal output with cursor save/restore. Some terminals
restore SGR attributes too. Previously `markdown::write_ansi` wrote SGR and text
as separate surface writes, so the attributes could be restored before the text
arrived. Styled rows now use one write containing both SGR and text, including
the line's inherited style. Row buffer capacity uses span lengths without an
extra text allocation. This also reduces terminal write calls.

Command, result and status rows now retain their semantic colors during live
streaming. Tool blocks have a separating blank row; model prose keeps the native
Markdown palette, and reasoning remains separately indicated in the status row.
The same table renderer remains active in streaming and transcript views.

Ordinary scroll mode has no automatic tool-ID/expand footer. Explicit tool
expansion and spill inspection are still available. Turn usage, request context
and completion statistics are stored as display-only metadata and wrap in replay;
they never enter model messages. Live readers receive those statistics as well.
Failure remains the last saved record so retry detection stays intact.

A tool preview's 360-byte truncation now respects Unicode character boundaries.
Previously an emoji straddling that boundary could panic.

## Verification

New unit cases enforce one terminal write per styled row, multiline summary
replay, and Unicode preview truncation. `scripts/smoke-presentation.py` checks
actual terminal cell colors (not just the presence of ANSI bytes), table output,
summary replay, no tool-ID footer, and composer input at 80x30, 40x24 and 20x8.
GPT-6 Luna medium independently reviewed correctness and performance; no blocker.

Final verification: strict Clippy, 621 workspace tests (520 core, 13 mod API,
88 TUI; one ignored doctest), actual release presentation PTYs, six empty-response
wire/PTY cases, and all 46 tool-repair cases passed.

Release SHA256:
`cd77c625c2f165a4f6ae43fe25fe0578c8f43f21a3b12e4d2abe7c0d01f88c99`.
Performance gate passed: worst median key visibility 0.36ms, redraw 0.28ms,
12,000-record reader frame 84.83ms, search 1.98ms. Statistics are stored for new
turns; prior releases did not save their ephemeral totals for retroactive replay.

Explicit search selection retains output-load guidance, with a human-readable
loaded/capped/failure status and no internal call ID. The reader smoke now checks
that human-readable status instead of requiring an implementation ID in the UI.

All 15 release regression suites passed with the final artifact unchanged:
polish, reader, inline-scroll, scroll-submit, native-files-jobs, context, blocks,
motion, live, recovery, permissions-pty, security, onboarding, mobile, features.
