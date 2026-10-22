# Native output expansion review

## Changes

The transcript no longer reserves or draws a vertical scrollbar. Position text,
paging, search, reading anchors, and follow controls remain available. Codex's
pager source was inspected at commit
`6b991972c5b539198dcea9bac4fc14e57559c3af`; GC retains its own screen controller.

Eight additional Mermaid families have bounded native layouts: pie, XY,
timeline, mindmap, journey, quadrant, Gantt, and Sankey. Unsupported grammar
retains its source. These layouts preserve values and relationships; they do
not claim browser rendering parity. Supported subsets are in `docs/terminal.md`.

Snippet highlighting now covers additional language aliases, multiline literals,
patches, configuration, and log severity. Display math supports matrices, cases,
aligned expressions, and stacked fractions. Narrow matrices use numbered rows.
Callouts have distinct headings. All rendering remains native, without new
runtime dependencies or rendering subprocesses.

## Independent review

One GPT-6 Luna reviewer checked the changes. A shared C-family triple-quote state
bug was fixed and covered by regression checks. The reviewer confirmed the fix.
A proposed Sankey header issue was retracted after checking Mermaid's parser:
records are source, target, value, without a header production.

## Verification

- Workspace tests: 992 passed; two existing integration tests remain ignored.
- Release diagram and Markdown PTYs: 69 passed across 80, 40, and 20 columns.
- Release snippet, math, streaming/replay, and exit PTYs: six passed.
- Native file throughput checks passed; 8 MB reads took a median 20 ms and
  approximately 1 MB edits took 27 ms on this host.
- Performance budgets passed: help startup 1.21 ms, interactive startup 6.01 ms,
  visible key update 0.14 ms, redraw 0.31 ms, and worst median reader frame
  for 12,000 history records 149.32 ms. Peak history child RSS was 4,252 KiB.
- Release binary: 6,821,264 bytes, unchanged from the installed baseline.

Measurements describe this host and fixture; they are not cross-platform bounds.
