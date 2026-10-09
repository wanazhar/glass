# Native output and structured tool blocks

2026-10-09

## Delivered

- Completed tool calls group outcome/duration, command, three indented preview rows, and remaining source-line count. Failure headings retain exit codes. Compact mode keeps failures visible. Full commands and results remain in `/details` and expandable transcript cards.
- Running command activity persists across parallel call completions; call IDs associate each result with its own command and duration.
- Native Markdown includes colored heading markers, framed code, nested bullets, task checks, and pipe-framed tables. Wide tables use columns; narrow tables use labeled fields. Unsupported tables remain literal text.
- Streaming table rendering holds one candidate header and bounded segments, then emits each complete data row. Exact source records/copies remain unchanged. No new dependency or provider response-format requirement.
- Assistant search uses rendered Markdown rows so table fields remain reachable.

## Verification

- Workspace tests: 585 pass (491 core, 11 mod API, 83 TUI); one doctest ignored. Strict all-target Clippy passes.
- PTY smoke-blocks: 90x30, 40x24, 20x8; first table row visible before response completion, colors/code frames, exact source, search, draft preservation and resize.
- PTY smoke-chat and smoke-live: five sizes through 20x8; command/result grouping, exact remaining count, errors, cancellation and continued tool activity.
- PTY smoke-inline-scroll: five sizes; exact cells/styles after wheel round trip, reachable tail, retained draft, one owned screen and full divider.
- Release renderer benchmark: 10,000 table rows (548,933 source bytes), widths 20/80. Layout 75–98 ms, streaming 84–143 ms on this host; all bounded benchmark assertions pass.

- Final release smoke-blocks and smoke-chat pass at the same sizes. Release performance budgets pass with 12,000 history records: worst-size median key visibility 0.139 ms, redraw 0.290 ms, reader frame 77.421 ms; reader peak RSS 3,476 KiB. Earlier baseline reader frame was 87.335 ms, peak RSS 3,236 KiB; measurements vary with host load.

## Limits

- Tables require outer pipes, support at most 32 columns, and use bounded 8 KiB candidate lines. Width allocation is stable rather than scanning all rows. Narrow layouts intentionally stack fields.
- Preview line count measures source lines; a partially shown long line uses a more-output hint instead of claiming an exact hidden-row count.
- Tool output is literal text, not interpreted Markdown. Full source remains available for inspection.
- Workspace release executable is `target/release/gc`; installed PATH binary is preserved. No push or publication.
