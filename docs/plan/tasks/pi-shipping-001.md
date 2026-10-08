---
id: pi-shipping-001
scope: regression review of 0954ab1 and 41b71a1
status: done
depends-on: []
---

## Objective

Verify newly shipped durability, context editing and program status. Fix confirmed
contract violations and preserve existing user edits and durable session data.

## Context

Read durability-001.md, context-edit-001.md and program-status-001.md.
The session repair contract preserves every complete record and permits safe
subsequent appends. A valid final record without its newline must not merge
with the next append. Torn-tail truncation must use the original byte boundary,
including blank lines and CRLF, rather than a normalized record length.

## Paths

- crates/glass/src/session.rs and regression tests for confirmed repair defects.
- Additional confirmed defects will be recorded before implementation.
- docs/plan/reviews/pi-shipping-001-01.md: independent verification evidence.

## Verification

Deterministic repair fixtures, workspace tests, strict Clippy, fresh release,
all smoke suites, and new feature PTY checks at 40x24 and 20x8.
Use disposable local mock providers, with no account or default install changes.

## Additional confirmed recovery defects

- Stale-partial checks must compare text only against an assistant recorded in
  the same raw user turn as the sidecar. Repeated text from a previous turn
  must remain recoverable. Remove a recoverable sidecar only after checked log
  writes succeed, preserving it when recording fails.
- Context edits must preserve compaction. An edit currently selects all raw
  records and bypasses compaction, reviving pruned context. Apply edits with
  original turn numbers while rebuilding the compaction projection. Stored
  summaries/state must not retain hidden or replaced content when that content
  was included in their source: fall back to a bounded edited context rather
  than reuse a stale summary or send the full raw history.

## Result

Confirmed repair, partial-recovery and compaction defects are fixed. Independent
verification passed 509 workspace unit tests, strict Clippy, release build and
all 32 smoke suites. Context edits and program status also passed additional
40x24 and 20x8 PTYs. See [the review](../reviews/pi-shipping-001-01.md) for
binary hash and test limitations. Existing user preference edits are preserved.
