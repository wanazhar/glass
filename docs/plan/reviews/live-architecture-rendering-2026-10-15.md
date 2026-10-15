# Live architecture rendering

Date: 2026-10-15.

## Reproduction

The installed GC was driven through a real 80x30 PTY using the selected Kilo via
SW connection (`kilo-auto/free`). The prompt asked it to study this repository
and draw the main crates and runtime components as Mermaid without modifying
files. Session storage was isolated under `/tmp/gc-architecture-live-data`.

The returned flowchart parsed successfully in this run. However, groups and
cycles selected the compact fallback, which displayed separate Nodes, Groups,
and Connections inventories. That output preserved the graph but did not provide
a useful architecture drawing. This reproduction does not prove that every
reported source fallback has the same cause.

The exact diagram is retained in
`scripts/fixtures/mermaid-architecture-kilo.mmd`. It includes an initialization
comment, nested groups, HTML label breaks, long labels, and cyclic connections.
The fixture records model output for rendering checks; it is not authoritative
architecture documentation.

## Changes

At content widths of at least 16 cells, compact flowcharts now draw a box for each
component. Group paths appear above their components, and each outgoing arrow is
attached directly below its source box with an explicit target reference. Label
breaks become separate body lines. Dotted and thick arrows retain their styles.
Cycles use references rather than recursive traversal or overlapping lines.
Quoted subgraph titles are accepted as well.

This is a vertical terminal layout for wide, grouped, or cyclic graphs. It does
not imitate a browser's spatial Mermaid layout. Views below 16 content cells
retain compact references; unsupported syntax still preserves source. The graph
size and output bounds remain in place to protect interactive performance.

No dependency, subprocess, network request, cache, or animation timer was added
to rendering. The renderer works within the existing node, edge, group, and
output limits. Original messages and logs remain unchanged.

## Verification

The exact Kilo diagram has a unit regression at seven widths from 16 through 88
cells. It checks one box per component, every connection, group labels, no raw
HTML, and row widths. A quoted-group regression and the existing cyclic,
fan-out, edge-style, and earlier-node membership cases cover adjacent behavior.
The release PTY audit includes this captured diagram at 80x30, 40x24, and 20x8,
with a boxed-component assertion before response completion and replay/search
checks afterward.

A GPT-6 Luna medium reviewer inspected width arithmetic, group paths, cycle and
fan-out handling, edge preservation, and performance bounds. It found no concrete
blocker. The reviewer did not edit files or run builds.

The final workspace passed 961 unit tests (838 core, 13 extension API, 1 runtime
client, 109 TUI), with two existing explicit ignores. Clippy passed with warnings
denied. All 42 release rendering PTYs passed, including the captured Kilo diagram
at all three sizes. Presentation and exact inline scroll/style round-trip checks
also passed.

The performance gate passed with 12,000 history records: worst viewport medians
were 6.41 ms to the prompt, 0.106 ms to show a key, 0.375 ms to redraw, and 125.74 ms
to show a reader frame. Peak reader RSS was 4132 KiB. These are local measurements
without provider latency. Binary size remains 6,493,584 bytes.

The verified release was installed atomically, preserving the relative `gc`
symlink. The actual, unmodified Kilo conversation was then resumed in the installed
binary at 80x30 and 40x24; search reached the diagram and verified boxed native
components. No additional model request was needed for replay. Evidence is under
`/tmp/gc-architecture-*`, including installed replay snapshots.
