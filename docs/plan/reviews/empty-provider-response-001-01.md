# Empty provider responses and transcript error styling

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

2026-10-09. The reported session contained reasoning deltas immediately before
its generic empty-response failures. The old adapter discarded completion reason,
response identity and shape information, so historical records cannot establish
the upstream root cause more precisely.

## Changes

- Error metadata wraps fully in red in transcript mode. System errors retain
  red without the dim modifier. The live turn prints one canonical persisted
  error instead of a failure banner plus a duplicate error.
- Empty OpenAI-compatible completions report event count, finish reason, model,
  response ID, reasoning character count, reported token usage and delta field
  names. No prompt, tool output or reasoning text enters these diagnostics.
  Existing API-key redaction and display sanitization remain in force.
- Reasoning-only answers get one corrective request per user turn, preserving
  completed tool results. The correction prompt exists only during that retry.
  Budget wrappers still authorize each request; it is not a free retry. Length
  and content-filter finishes have specific advice rather than blindly retrying.
- Usage events survive empty-response failures for provider billing wrappers.
- Compatible typed text parts and refusal strings render as visible answers.
- `/context` inspection starts with the latest provider failure, before the
  request manifest, so diagnostics are discoverable without searching history.

## Verification

Strict Clippy and 618 workspace tests passed (519 core, 13 mod API, 86 TUI;
one ignored doctest). New coverage includes diagnostic fields, no reasoning-text
leakage, refusal/typed text, and full red error wrapping.

The provider-wire smoke covers empty answers, repeated reasoning-only responses
(two requests maximum), recovery after a successful tool (side effect exactly
once), context diagnostics and red transcript replay at 80x24, 40x24 and 20x8.
GPT-6 Luna medium review caught a per-turn reset bug; fixed by resetting recovery
allowance and restoring the original prompt after the corrective request.

Final release SHA256:
`813ae9f747086abb9b65e5a5e368a68c91a2242db3be673471a52ea226e9168d`.
The final artifact passed the six new wire/PTY cases, all 46 tool-repair cases,
persistent teammate integration/three terminal sizes, and the performance gate.
Worst median key visibility was 0.91ms, redraw 0.86ms; 12,000-record reader
frame 89.12ms. No dependency or extra background polling was added.

A real saved Kilo/SW release probe ran a requested printf command and returned a
final answer successfully. A separate read-only replay of a copy of the complete
reported session timed out at 150s; it is not evidence that the upstream problem
is solved for that history. Original session files were preserved. The exact
upstream cause remains unconfirmed; new diagnostics distinguish future cases.

All 15 final release regression suites passed with the artifact hash unchanged:
polish, reader, inline-scroll, scroll-submit, native-files-jobs, context, blocks,
motion, live, recovery, permissions-pty, security, onboarding, mobile, features.
Final independent review found no blocking recovery lifecycle issue. The
correction hint is appended after the effective system checkpoint, so compaction
cannot silently remove it; it is cleared on both success and failure.
