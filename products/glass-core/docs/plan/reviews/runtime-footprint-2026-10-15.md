# Runtime footprint and performance — 2026-10-15

## Changes

Foreground shell capture retains 16 KiB inline and 4 MiB of overflow per stream.
It drains further output, reports discarded bytes, and includes a 2 KiB final
tail. A short-read bug that wrote unused buffer bytes into overflow is fixed.
Completed overflow moves into content-addressed storage without a duplicate
copy. Publication replaces a corrupt existing blob atomically and refreshes the
grace period before its new reference is recorded.

Explicit blob sweeping now scans foreground, one-shot subagent, and persistent
teammate logs. Reads use 32 KiB buffers with boundary overlap. Symlinked,
non-regular, unreadable, or excessive reference trees prevent deletion. This
corrects the previous omission of nested worker references.

Overlay output uses the existing bounded display cache rather than a disk spool.
Character-width checks no longer allocate a temporary string per character.
Default system and tool descriptions are shorter; detailed guidance stays on
demand. Generated workflow and permission paragraphs replace earlier versions
on resume, while custom instructions remain intact. The release profile uses
size optimization with full LTO and stripped symbols.

## Footprint measurements

Baseline: installed binary from `2cecda9`. Comparison: the release build for this
change, on the same Linux aarch64 host with Rust 1.97.0. The offline footprint
script sends a
greeting and a command producing 6,000,000 repeated bytes plus a final marker.
Each scenario uses disposable configuration/data and a local HTTP provider.

| Measurement | Baseline | Updated | Reduction |
| --- | ---: | ---: | ---: |
| Executable | 7,673,192 bytes | 6,231,400 bytes | 18.8% |
| Cold system instructions | 2,766 bytes | 1,811 bytes | 34.5% |
| Declared tool schemas | 7,220 bytes | 6,761 bytes | 6.4% |
| Combined cold system/schema content | 9,986 bytes | 8,572 bytes | 14.2% |
| Verbose tool result | 60,176 bytes | 18,733 bytes | 68.9% |
| Verbose scenario automatic storage | 12,015,178 bytes | 4,237,125 bytes | 64.7% |

The greeting stored 2,699 bytes in each build. No model request-body trace was
stored. These are byte counts, not tokenizer counts or billing estimates.
Additional file reads can offset tool-output savings on tasks that need the
omitted text. Provider caching, reasoning, latency, and coding quality were not
measured by this fixture.

## Timing measurements

Three alternating runs of each binary used `check-performance.py` followed by
`check-native-performance.py`, after compilation and other fixtures stopped.
Values below are medians of the three reports. PTY results use the worse of
40×24 and 20×8 where applicable. Files and prompts were local.

| Measurement | Baseline | Updated |
| --- | ---: | ---: |
| Interactive startup | 5.63 ms | 5.85 ms |
| Typed key visible | 0.136 ms | 0.131 ms |
| Visible redraw | 0.290 ms | 0.334 ms |
| Reader frame, 12,000 records | 116.89 ms | 102.51 ms |
| Read 7,999,992 bytes | 17.88 ms | 18.42 ms |
| Edit approximately 1 MB | 25.17 ms | 25.78 ms |

The reader improved by about 12%. Startup and native operations show small
slowdowns in this sample; this change does not claim that every operation is
faster. The smaller binary is retained with those measured tradeoffs. These
results do not predict performance on another machine or endpoint.

## Verification

The workspace passed 931 unit tests; one existing doctest remains ignored.
Clippy passed with warnings denied. Release fixtures passed live reasoning,
continued tools, overlay resize without UI spool files, exact scroll cells and
styles, native checklists, structured questions, one-call approvals, headless
behavior, and file confinement. Small-screen PTYs include 20×8.

The release footprint gate checked bounded storage, final-tail disclosure, and
the absence of duplicate output files. A separate release CLI fixture verified
that blob sweeping retains nested worker output and refuses unsafe reference
scans before deleting anything. One GPT-6 Luna reviewer at medium reasoning
reviewed the changes and follow-up fixes; no blockers remained.

See [Runtime storage](../../reference/storage.md) for current retention limits
and reproduction commands. Conversation history is not automatically expired,
and the application has no global storage quota. Existing user data was not
deleted by this change.
