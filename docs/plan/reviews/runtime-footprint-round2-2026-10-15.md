# Runtime footprint optimization: round 2

Date: 2026-10-15. Host: Linux aarch64, Rust 1.97.0.
Baseline: d7486a3. Candidate: working tree committed with this record.

## Changes

- Use one-allocation hexadecimal encoding for SHA-256 identifiers. Message
  fingerprints retain the existing framing, including empty fields.
- Preallocate bounded native file reads and reuse calculated revisions.
- Return one executable JSON continuation for truncated native reads. The CLI
  retains its human-readable continuation hint.
- Shorten repeated request-journal explanations.
- Bound prompt recall to 1 MiB and 64 KiB per encoded entry. Preserve the latest
  200 visible entries when they fit; compact after 232 physical lines. Use
  nonblocking private locks and atomic compaction. Conversation logs are unchanged.
- Decode only the newest recall line on ordinary appends. A torn final record
  cannot suppress a later repeated prompt.

## Measurements

Three alternating comparisons used the installed baseline and actual release
candidate. UI figures are medians of the worst reported terminal-size median.

| Measurement | Baseline | Candidate |
| --- | ---: | ---: |
| Binary bytes | 6,231,400 | 6,231,400 |
| Interactive prompt | 6.05 ms | 5.75 ms |
| Typed key visible | 0.119 ms | 0.121 ms |
| Visible redraw | 0.298 ms | 0.326 ms |
| Reader frame, 12,000 records | 103.07 ms | 104.29 ms |
| Read 7,999,992-byte file | 19.05 ms | 17.31 ms |
| Edit approximately 1 MiB | 25.99 ms | 24.13 ms |
| Partial-read model result | 391 bytes | 253 bytes |
| Greeting automatic storage | 2,699 bytes | 2,473 bytes |
| Partial-read automatic storage | 5,498 bytes | 4,905 bytes |

The recall stress fixture preseeded 200 prompts of approximately 8 KiB and
submitted 40 new turns through a real PTY. Process-written characters fell from
61,148,217 to 3,213,590 (94.7%). This counter includes terminal, network, and
diagnostic writes as well as recall. Final recall storage fell from 1,312,610 to
1,041,944 bytes. The bounded journal retained 167 physical entries because large
prompts consume the byte allowance. Full conversation logs were preserved.

Cold system instructions (1,811 bytes) and tool schemas (6,761 bytes) did not
change. Payload bytes are a token-cost proxy; provider billing was not measured.
These local fixtures do not establish performance on other machines. Small UI
timing differences are reported without claiming a general rendering speedup.

## Verification

- 938 Rust tests passed; one existing doctest remains ignored.
- Clippy passed with warnings denied.
- The actual release passed live streaming, cancellation, resize, exact scroll
  cell/style round trips, native checklist, approvals, questions, and provider
  pacing fixtures, including 20×8 terminals.
- Tooling foundation passed 15 CLI edges and PTYs at 80×30, 40×24, and 20×8.
  Its stale exact tool-list assertion was updated to the seven exposed tools.
- Runtime storage, recall write-traffic, native throughput, and UI budgets passed.
- One independent GPT-6 Luna medium reviewer checked the changes. Its torn-tail
  finding was fixed and covered by a regression test; final review had no blockers.

Evidence is retained locally under /tmp/gc-opt2-*.json and /tmp/gc-opt2-*.log.
