# Native output expansion

Status: complete. Review and measurements: [output-polish-2026-10-16.md](reviews/output-polish-2026-10-16.md).

## Contract

Performance comes first: no new rendering service, grammar runtime, browser,
or TeX dependency. Parse bounded input, retain exact source, and share rendering
between streaming and replay. Unknown or ambiguous syntax falls back to source
with a readable explanation rather than silently dropping content.

Expand Mermaid with pie proportions, XY data, timelines, mindmaps, user journeys,
quadrants, Gantt schedules, and Sankey flows. Use terminal layouts that preserve
values and relationships. Do not imitate a browser chart where terminal cells
cannot represent it accurately. Charts must remain readable at phone widths.

Expand language aliases, multiline literals, diff/config/log styling, and code
framing. Expand mathematical operands, roots, symbols and structured display
matrices/aligned expressions. Inline math must not eat ordinary currency text.

Remove the vertical transcript scrollbar. Preserve paging, anchoring, follow mode,
position text and the return-to-bottom hint. Use the freed column for content.
Codex source checked at commit `6b991972c5b539198dcea9bac4fc14e57559c3af`:
[`pager_overlay.rs`](https://github.com/openai/codex/blob/6b991972c5b539198dcea9bac4fc14e57559c3af/codex-rs/tui/src/pager_overlay.rs)
uses offset navigation and a bottom progress strip without a vertical scrollbar.
The native scrollback strategy is in `codex-rs/tui/src/tui/scrollback.rs`.
GC keeps its owned screen and existing scroll controller.

```text
Conversation — line 24/100 ──────────────────
  heading
  code / chart / math using available width

Enter send draft · End/Esc bottom
────────────────────────────────────────────
❯ draft
```

## Verification

Use exact-source and bounded-width tests, fragmented-stream/replay comparisons,
invalid-input fallbacks, numerical/date edge cases, and release PTYs. Compare
binary size and startup/input/reader performance with the installed baseline.
Root implements directly; one Luna medium reviewer checks the final changes.
