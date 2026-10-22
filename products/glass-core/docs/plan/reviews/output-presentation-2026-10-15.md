# Output presentation review

Date: 2026-10-15. Scope: interactive GC output and provider reasoning.

## Changes

Tables distribute unused cells between columns, pad rows to the same width as
headers, and connect dividers at column boundaries. Standard separator markers
select left, centered, or right alignment. Narrow tables retain labeled fields.
No complete table is buffered; each completed row is rendered independently.

Reasoning opens at the latest lines on every invocation. Reopening a parked
reader clears its search, refreshes its entries and terminal state, and then
paints the latest position. It does not paint the previous scroll position first.
A live label identifies incoming reasoning. Scrolling upward pauses following;
End resumes it. Esc closes the panel. Reasoning uses the existing native Markdown
renderer, viewport cache, and event loop.

The wider presentation pass also fixed:

- Code-fence prefixes exceeding one- and two-cell content widths.
- Missing native rendering for Markdown horizontal rules.
- Rendered Mermaid diagrams hiding surrounding prose or later blocks.
- Detailed tool output losing the colors used by its compact preview.
- Long running-tool titles exceeding their viewport.
- Saved-output and truncation notices exceeding narrow tool cards.

Headings, quotes, emphasis, nested lists, task checks, fenced code, Unicode text,
and streamed/replayed tables were checked together. Rendering remains a display
operation: original messages, copied code, and durable logs are unchanged.

## Verification

949 Rust unit tests passed, including new coverage for table layouts with two
through eight columns across widths 1 through 100, divider joins, alignment,
small code blocks, diagram context preservation, detailed tool colors, and
bounded tool titles/notices. Stream/replay tests now include quotes and rules at
nine widths. Clippy passed with warnings denied.

Release PTYs cover streamed native blocks at 90, 65, 41, 40, 39, and 20 columns;
reasoning reopening and navigation at five sizes down to 20 by 8; semantic colors;
exact scroll-cell/style round trips; resize; and preserved drafts. The reasoning
fixture verifies that scrolling to the beginning, closing, and reopening shows
the latest lines again. Its paused footer identifies End as the way to return
to the latest lines.

One GPT-6 Luna medium reviewer checked the source changes and the final
reasoning-reopen sequence. It reported no remaining blocker.

Local evidence is retained in /tmp/gc-output-*.log. PTY table and reasoning
snapshots are written to /tmp/gc-output-table-* and /tmp/gc-reasoning-latest-*.

## Performance boundary

No dependency, model request, disk spool, or animation timer was added. Completed
table rows retain bounded parsing. Diagram lookahead stops after 256 lines or when its candidate exceeds
128 KiB; failed or incomplete diagrams fall back to source rendering. The
existing viewport and event cadence handle reasoning updates. Conversation
reading positions are preserved; only reasoning invocation deliberately resets
to the latest content.

The final release passed the local performance gate. Worst viewport medians were
5.64 ms to the interactive prompt, 0.142 ms to show a key, 0.337 ms to redraw,
and 108.10 ms to show a reader frame with 12,000 records. Peak reader RSS was
4132 KiB. These are machine-specific measurements without provider latency.
The executable remains 6,493,584 bytes. The installed executable is byte-identical
to the tested release, and its presentation PTYs passed at 80 by 30, 40 by 24,
and 20 by 8. Host-runtime conformance and the Mermaid source-preservation smoke
also passed.
