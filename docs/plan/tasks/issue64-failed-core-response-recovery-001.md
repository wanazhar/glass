---
id: issue64-failed-core-response-recovery-001
scope: issue-64-failed-core-response-recovery
status: complete
depends-on: [issue64-send-attempt-recovery-001, issue63-attention-inbox-001]
---

# Failed Core response recovery

## Objective

Make a Core-confirmed failed prompt or follow-up visible and actionable without
confusing it with an uncertain delivery, replaying it automatically, or
exposing a raw provider error to the conversation UI.

## Paths

- `crates/glass-dev/src/agents.rs`
- `crates/glass-dev/src/tui/projection.rs`
- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/render.rs`
- `crates/glass-dev/src/tui/command.rs`
- `crates/glass-dev/tests/fixtures/fake_glass_core_runtime.py`
- `crates/glass-dev/tests/tui_pty.rs`
- `docs/development-runtime.md`
- `docs/cli.md`

## Acceptance

- [x] Only an error response correlated to the current prompt/follow-up
  request ID marks that attempt failed and returns its run to idle.
- [x] The conversation displays a bounded summary from the allowlisted Core
  error class, integer code, and retryable flag. It never renders the raw
  provider/Core error message.
- [x] Attention Inbox shows confirmed Core failures separately from uncertain
  deliveries. A failed attempt without a Core request ID is not promoted into
  this recovery path.
- [x] Opening either item does not resend it. A failed attempt can be retried
  only after inspection and an explicit confirmation; cancellation preserves
  the failed attempt and sends no request.
- [x] Confirmed retry preserves the client message ID and creates a distinct
  attempt ID while retaining the prior failed record.
- [x] Fake-Core PTY coverage verifies a correlated error reaches the active
  conversation, its raw message stays hidden, the failure reaches Attention
  Inbox, and retry requires confirmation. A separate persisted-ledger journey
  verifies review, cancellation, and confirmed retry.

## Evidence

`failed_core_response_is_visible_and_can_be_retried_explicitly` exercises the
runtime response path, safe conversation summary, attention routing, and
confirmation gate. `failed_core_turn_is_reviewable_and_retry_requires_confirmation`
starts from a persisted failed attempt, resumes the owning session, cancels
retry once, then confirms a second retry and verifies the original failure and
new settled attempt retain distinct attempt IDs and a shared client message ID.
The offline projection test proves that a raw fixture provider message is
omitted. Focused glass-dev tests, PTY compilation and journeys, strict scoped
Clippy, formatting, and diff checks pass.

The live-provider composer journey remains incomplete and is tracked separately
from the offline recovery proof. An earlier integrated attempt ended in HTTP
429 capacity retries before a tool call or file edit. A later bounded attempt
reported a correlated send attempt as `failed`, its run as `idle`, no output
file, and no observed HTTP 429; at that time the safe error was not visible in
the PTY. After the error projection and class detector were updated, one more
bounded attempt again reported `failed`/`idle`, a correlated request ID, no
file, and no observed 429, while the PTY detector still reported no recognized
safe class. The attempts are separate observations; the later non-429 result
does not identify the cause of the earlier capacity failure. No automatic
retry was attempted. The direct Core readiness/provider probe and a direct
Core response had succeeded earlier, but they do not prove coding through the
Glass composer. The current live failure class and cause remain unknown; the
offline fixture proves only the host error-projection and recovery behavior.
