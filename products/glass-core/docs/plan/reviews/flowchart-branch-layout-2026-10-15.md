# Flowchart branch layout

Date: 2026-10-15.

The reported screenshot used the old wide-flow renderer. That path repeated
nodes at successive ranks and appended every outgoing edge to the same text row.
Two separate branches could therefore look like a serial chain. Earlier changes
replaced only the compact fallback, leaving this path active for small graphs.

The wide serializer is removed. Branches, cycles, state, class, and ER diagrams
use boxed components with source-attached edges. Each edge occupies its own
wrapped region; its label stays with its arrow and destination. State aliases
appear in box headers rather than internal state identifiers.

A single connected solid flowchart chain uses centered boxes and vertical
connectors. Eligibility requires exactly N-1 edges joining adjacent declared
nodes, no groups, nonempty labels, and solid arrows. Other graphs keep explicit
destination references. This avoids recursive traversal, false junctions, and
unbounded routing work. Dotted and thick edges retain their distinct styles.

Native diagram output now uses an accent heading and a spacer instead of an
incomplete outer frame around already framed components. Node frames and arrows
use the shared terminal palette. Original source, copied code, and logs are
unchanged. Existing byte, node, edge, width, and output bounds remain in place.

The screenshot's five-node decision/merge graph has a direct regression checking
one box per node, no serialized multi-arrow row, all five connections, both
branch labels, and row width at 20, 40, 65, and 88 cells. The release PTY fixture
includes the same graph at 80x30, 40x24, and 20x8, alongside the previous live
Kilo architecture fixture and supported diagram families.

A GPT-6 Luna medium reviewer inspected the removal, chain eligibility,
source/target association, edge styles, and width/output bounds. It found no
concrete blocker and did not edit files or run checks.

The workspace passed 962 unit tests (839 core, 13 extension API, 1 runtime client,
109 TUI), with two existing explicit ignores. Clippy passed with warnings denied.

All 45 release rendering PTYs passed at 80x30, 40x24, and 20x8. Presentation,
exact inline scroll/style round trips, and Mermaid source preservation passed.
The performance gate passed with 12,000 history records: worst viewport medians
were 6.27 ms to the prompt, 0.164 ms to show a key, 0.314 ms to redraw, and
118.03 ms to show a reader frame. Peak reader RSS was 4000 KiB. These local
measurements exclude provider latency. Binary size remains 6,493,584 bytes.

The verified release was installed atomically with the relative `gc` symlink
preserved. Evidence is under `/tmp/gc-flow-layout-*`; branch snapshots are under
`/tmp/gc-render-audit-branches-*`.
Installed-binary branch PTYs also passed at all three sizes.
