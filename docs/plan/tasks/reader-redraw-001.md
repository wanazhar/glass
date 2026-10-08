---
id: reader-redraw-001
status: complete
scope: clean reader frames and canonical history presentation
---

## Contract

First entry into the inline reader must replace every owned body cell, including blank cells, without changing pinned composer/status or opening another alternate screen. Reentry, paging, resize and overlays must not retain text from an earlier renderer. Render using the full body viewport without terminal scroll-margin interference.

Main conversation history must not show each reasoning delta as a separate tool row. Provider-exposed reasoning stays in its dedicated thinking view. Present each tool execution once with readable command/status/output, rather than duplicated raw JSON call, Bash echo and ToolResult. Keep original JSONL, exact copies, model replay and tool inspection unchanged. Handle repeated identical commands and legacy Bash-only sessions without dropping real executions. Display non-Bash tool calls/results coherently and preserve errors.

## Gates

Independent real PTY at five existing sizes with a body prefilled with distinctive long lines, then short/blank reader rows: assert old sentinel cells vanish across the complete body, composer remains intact, resize/page/modal restore and terminal cleanup pass. Synthetic session must include fragmented Reasoning, structured Bash call/Bash echo/result, repeated identical executions, non-Bash calls, failures and compaction; assert canonical rendering, retained exact records and reasoning-view accessibility. Focused tests and Clippy. Rebuild the frozen release inside /tmp and test that exact artifact. Live provider gates remain explicit if networking is denied.

## Implementation

All fresh inline-reader terminal instances clear only the owned body rectangle before their initial diff. This includes opening, resizing and restoring after overlays. Independent baseline reproduction injected visible sentinel rows into the PTY output and demonstrated old cells surviving the original first frame.

History presentation operates on copied records and leaves persisted JSONL/model replay untouched. Tool identity is scoped by user turn, compaction epoch and call ID; ambiguous duplicate IDs remain visible. Bash echoes are paired sequentially to matching commands within that scope, preserving repeated executions and Bash-only legacy history. Public reasoning chunks are excluded from main entries and concatenated for the latest-turn thinking view, including resumed sessions.

Focused session tests (14), layout tests (16) and scoped strict Clippy pass. Independent updated-debug and exact installed-release PTY checks pass at 65x30, 40x24, 24x16, 20x12 and 20x8 with poisoned body rows, mixed tool history, reasoning-view navigation, preserved drafts/cursor and unchanged JSONL SHA256. See ../reviews/reader-redraw-001-01.md. Live-provider streaming was not exercised; this pass covers the offline rendering/history regression.

Supported offline release installation completed at `/tmp/gc-final-install/bin/gc -> glass-core`; log `/tmp/gc-reader-redraw-install.log`, binary SHA256 `b8f5d8bb39f126f2592bfea5aa51b177b8d31b4d83f8ab7fe9d23a1f2c07428b`. Default user installation remains outside writable roots. The separate Glass executable checksum is unchanged.
