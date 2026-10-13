# Release binary: 15 edge-case suites and live Kilo task

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Date: 2026-10-09. Actual binary: `/home/ubuntu/work/glass-core/target/release/glass-core`; `gc` is its symlink. All subprocess and PTY harnesses explicitly received `GLASS_CORE_TEST_BINARY`. Tests use isolated temporary configuration and workspaces; no installed executable or user settings were replaced.

## Matrix

| Suite | Edge coverage | Final result |
| --- | --- | --- |
| `polish` | Escape, unmatched slash filters, resize and draft preservation | PASS |
| `reader` | Scrollbar endpoints, full-output search, cards and activity actions | PASS |
| `inline-scroll` | Wheel scrolling, top/tail, resize and composer preservation | PASS |
| `scroll-submit` | Submit and steer while scrolled, outside Git | PASS |
| `native-files-jobs` | Revision edits, diagnostics, job stdin/cursor/stop and non-Git CLI | PASS |
| `context` | Context audit and accurate token accounting | PASS |
| `blocks` | Native tables, framed code, streaming Markdown and narrow layouts | PASS |
| `motion` | Animated activity, busy drafts, cancellation and settled idle | PASS |
| `live` | Streaming tool output, reasoning indicators and live modal behavior | PASS |
| `recovery` | Crash mid-tool, partial output and torn-log repair without replay | PASS |
| `permissions-pty` | Permission approvals and refusal in the actual terminal | PASS |
| `security` | Read-only profile enforcement through the real bash path | PASS |
| `onboarding` | Provider/model setup, authentication and error paths | PASS |
| `mobile` | Raw-terminal staircase regression and SSH-sized layouts | PASS |
| `features` | Input latency, limits, checkpoints, branching and job cleanup | PASS |

## Bugs found and fixed

1. Background polling converted split UTF-8 byte chunks independently, corrupting euro/emoji characters. Polls now extend at most three bytes to complete a scalar, and leave incomplete live characters at the same byte cursor. Final invalid bytes remain explicitly lossy; this is a text log interface. Completion requires `state: exited` and `draining: false`.
2. The dependency-free tool argument decoder rejected valid escaped UTF-16 surrogate pairs, so commands containing JSON-escaped emoji became “command required.” Decode valid pairs and reject unpaired or incomplete sequences.

Added unit tests and an actual-binary streaming-provider regression with split UTF-8 writes, one-byte polls and JSON-escaped emoji. Added the regression to CI.

## Verification

604 workspace tests passed, one ignored doctest; strict Clippy passed. GPT-6 Luna medium review found no correctness blockers after fixes. Poll output is bounded at 24 KiB plus at most three UTF-8 continuation bytes; no dependency or model-tool surface was added.

The first actual-release pass ran all 15 suites successfully. Additional boundary probes found the Unicode bugs; the final rebuilt release is rerun below. Logs and machine-readable results are under `/tmp/gc-release-15` (temporary local artifacts).

Live Kilo/SW task: in a temporary non-Git workspace, fixed `calc.py` addition using native read/revision-checked edit, then verified `add(2,3)==5` and `add(-2,3)==1` using Python. Recorded shell tool exits were zero. A live PTY greeting and slash-palette dismissal are checked after completion rather than matching the echoed user prompt.

Final release SHA-256: `8b91efdf34dbcddcc94e23d72256059944ebb9d58692298ab046a84d52d92bd2`. All 15 suites passed on this unchanged binary; split-Unicode regression and both performance scripts passed. Final live Kilo headless coding task completed successfully in 62.47 seconds, and the release PTY greeting/palette Escape check passed.
