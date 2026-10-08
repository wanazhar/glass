# Review: chat-readability-001

Reviewer: independent verifier. Date: 2026-10-07.

## Gate

PASS. Source, debug-binary and installed-binary verification passed. All six installed PTY suites and the independent readability suite passed. The extension cleanup assertion now waits up to three seconds for actual PID absence, and the installed workflows rerun passed. No unresolved blocking findings. The verifier confirmed this gate before its optional supplementary run was interrupted.

## Findings resolved during review

1. **Streaming resize lost or repeated response text.** Row-count offsets depended on terminal width. The renderer now advances by semantic word/character position; the grow/shrink regression covers the exact reported reproduction.
2. **Unfinished lines grew without a bound.** Pending display segments now roll over at whitespace after 4 KiB, with a hard threshold near 8 KiB. Original session records remain unchanged. Long no-newline regression covers retained memory and word fidelity.
3. **Unclosed code fences leaked into subsequent turns.** Flush resets presentation; main flushes at tools, retries, guard notices and turn completion.
4. **Asterisk lists waited for a whole line.** Stable inline buffering now recognizes asterisk list markers, including leading indentation, before scanning inline emphasis. Independent PTY holds the HTTP stream before the list newline and verifies visible list text.
5. **Terminal controls escaped through auxiliary output.** Guard paths, retries, interactive provider errors, BTW answers/errors and final stderr now pass through presentation or sanitation. Independent PTY injects OSC 52 into an HTTP error and verifies no executable OSC is emitted.
6. **Common nested markers remained visible.** Inline emphasis containing inline code and headings containing emphasis now use shared styling; independent PTY checks these patterns without raw double asterisks.
7. **Failed tool output lacked a useful error preview.** Latest source includes the first nonempty failure output line in the display summary while preserving full tool output for the model/log. Final developer PTY rerun passes the actual error preview.

## Independent evidence

- `/tmp/verify_gc_chat.py`, output `/tmp/gc-chat-review.log`: real PTY at 65×30, 40×24, 24×16, 20×12 and 20×8; grows 40 columns to 80 during an unfinished list.
- Verifies a list renders before final newline/completion, nested markers disappear, final partial text survives, original Markdown persists in JSONL, and provider error control bytes are filtered.
- `/tmp/gc-chat-design-65.txt` is a projected real PTY chat capture. Visual inspection of `/tmp/gc-chat-design-65.png` confirms readable margins, intact words, hanging bullet continuation, paragraph spacing and separated code. The final developer PNG projects captured ANSI bytes including styles and CR erase; it is a visualization of a real PTY capture, not a phone screenshot. Local font lacks CJK/emoji glyphs; source capture retains those characters.
- Final developer logs: `/tmp/gc-chat-pty.log` and `/tmp/gc-chat-tests.log` report the complete chat PTY suite and 374 unit tests passed; Clippy and mobile PTY pass.
- Shared Markdown uses controlled SGR output and never emits provider ANSI/OSC or activates links. Response copy/code extraction continue to use original records.

## Limits

The renderer implements a deliberately small Markdown subset rather than full CommonMark. Unicode widths are conservative per-character cell widths; complex joined emoji may wrap more conservatively than a terminal's grapheme rendering. Desktop/mobile terminal behavior is covered through real local PTYs; this does not directly emulate the user's SSH client font or scrollback behavior.

## Installed verification evidence

- `/tmp/gc-chat-installed-{features,onboarding,polish,workflows-bounded,mobile,chat,review}.log`: all required suites passed against installed gc.
- `/tmp/gc-chat-install.log`: release installation completed; gc is a relative symlink to glass-core.
- Separate Glass executable checksum remained unchanged.
