# Native rendering audit

Date: 2026-10-15. Scope: streaming output, conversation replay, native diagrams,
Markdown, search, and narrow terminal layouts.

## Findings and changes

Mermaid was disabled by default. Even when enabled, streaming treated the block
as code, while replay used a diagram callback. The flowchart parser also rejected
comments, class suffixes, groups, and chained connections used in the reported
architecture diagram.

Native diagrams now default to enabled. Completed streamed fences and replay use
the same renderer. Flowcharts accept comments, quoted labels, HTML line breaks,
class suffixes, nested groups, late label definitions, chains, and fan-out edges.
Solid, dotted, and thick arrows remain distinct. Groups include nodes declared
before the group when those nodes are referenced inside it. Wide or cyclic
flowcharts use compact node, group, and connection lists. Narrow sequence diagrams
use participant and message lists rather than dropping events.

The broader audit corrected missing underscore emphasis, strikethrough, nested
emphasis, visible link destinations, image references, multi-backtick inline code,
and literal spaces inside inline code. A character-by-character stream regression
also caught a partial-line boundary splitting a completed code span; completed
markup now retains its stable boundary. Numbered lists accept a closing parenthesis;
nested quote rails and indented continuation text retain their structure. Search
now measures assistant matches using the actual reading column.

## Coverage and limits

| Output | Presentation |
| --- | --- |
| Flowcharts | Ranked lanes or bounded node/group/connection lists |
| Sequences | Lifelines when they fit; participant/message lists otherwise |
| State, class, ER | Bounded structural views |
| Unsupported or malformed diagrams | Original fenced source with an explanation |
| Explicit `mermaid = false` | Source view |
| Tables | Aligned columns or labeled narrow fields |
| Headings, lists, checks, quotes, rules | Shared native Markdown rendering |
| Links and image references | Visible label and destination; no network fetch |
| Code fences and inline code | Source text with terminal-safe styling |
| Raw HTML and reference-link definitions | Text; no browser rendering |

This is a terminal subset of [Mermaid](https://mermaid.js.org/syntax/flowchart.html)
and [Markdown](https://spec.commonmark.org/0.31.2/). It does not claim full grammar
coverage. Pie, Gantt, frontmatter, interactive directives, full class members,
and ER attribute blocks retain source. Custom CSS is not executed; diagrams use
GC's theme. Display changes do not alter original messages, logs, or copied code.

## Performance boundary

No dependency, rendering subprocess, model request, disk cache, or timer was
added. Streaming buffers only a Mermaid fence, up to 128 KiB or 256 lines. Ordinary
prose and code retain their existing streaming path. Overflow and incomplete
fences fall back to source. Flowcharts cap nodes at 64, edges at 128, groups at 64,
and nested groups at 8. Native compact output is bounded to 2048 rows. Labels and
identifiers have separate size limits.

## Review and verification

A GPT-6 Luna medium source reviewer found two issues during review: lost dotted
and thick arrow styles, and incomplete group membership for earlier nodes. Both
were fixed and covered by a direct regression. The final source review found no
remaining concrete blocker.

The release fixture in `scripts/smoke-rendering-audit.py` covers 13 cases at
80x30, 40x24, and 20x8: the reported architecture syntax, cycles, fan-out, Unicode,
all five supported diagram families, malformed and unsupported source, explicit
disablement, and mixed Markdown. It checks rendering before response completion,
replay/search, exact durable source, draft preservation, and no extra model calls.

The workspace passed 959 unit tests (836 core, 13 extension API, 1 runtime client,
and 109 TUI), with two pre-existing explicit ignores. Clippy passed with warnings
denied. Streaming/replay parity is checked character by character at nine widths,
including links, image references, nested emphasis, repeated inline-code spaces,
quotes, lists, tables, and code fences.

The PTY emulator needed a test-only correction: pyte leaves an orphan continuation
cell after the leading half of a wide glyph is overwritten by a terminal diff.
The harness now treats that orphan as a blank, matching terminal behavior. Valid
wide glyphs remain intact; no product drawing workaround was added.

All 39 rendering PTYs passed against the final release binary. Existing native
block, live reasoning, presentation, exact inline scroll/style round-trip,
Mermaid source-preservation, and external host-runtime conformance checks also
passed. Updated documentation links resolve. Local evidence is under
`/tmp/gc-render-audit-*` and `/tmp/gc-render-verified-*`; snapshot text includes
`/tmp/gc-render-audit-screenshot-40x24.txt`.

The final release passed the local performance gate with 12,000 history records.
Worst viewport medians were 7.45 ms to the prompt, 0.066 ms to show a key,
0.286 ms to redraw, and 123.23 ms to show a reader frame. Peak reader RSS was
4004 KiB. These are local measurements without provider latency. The executable
remains 6,493,584 bytes, the same size as the preceding release. The installed
binary is byte-identical to the verified release; the relative `gc` symlink is
preserved.
Installed-binary presentation and reported-architecture diagram PTYs passed at
80x30, 40x24, and 20x8.
