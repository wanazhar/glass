# Final audit: target-009 deterministic element targeting

Reviewed branch through `90c1a0f` (`fix: arm pointer release before press`).

Conclusion: **pass**. The final P1 cancellation race is closed. No new blocking
correctness or security regression was found.

## Resolution

`PressedButtonGuard` is now created and armed immediately before the
`mousePressed` CDP dispatch is awaited (`src/browser/session.rs:886-904`). If
the operation is cancelled, times out, or receives a CDP error after Chrome may
have applied the press, stack unwinding drops the armed guard and schedules a
best-effort `mouseReleased`. A failed press may therefore produce a harmless
redundant release, which is the safe choice for an accepted-but-unacknowledged
input command.

The guard is removed and disarmed only after `mouseReleased` returns
successfully (`src/browser/session.rs:905-909`). If release dispatch itself is
cancelled or fails, the guard stays armed and retries release from `Drop`.
Double-click uses the same per-press lifecycle because each later
`mousePressed` installs a new guard after the prior acknowledged release.

This ordering covers the press-response window identified in
`target-009-03` while preserving press-boundary target revalidation, normal
human dwell, fast dispatch, and the existing real-Chrome cancellation test.

## Remaining non-blocking follow-up

The prior P2 error-path cleanup observation still applies:
`bounded_element_query` releases its remote array and child handles on success,
but `Runtime.getProperties` or an intermediate `DOM.requestNode` error can
return before the array and remaining handles are released
(`src/browser/cdp.rs:424-479`). This does not block the deterministic targeting
contract, but an object-group/finally-style cleanup should be added during
resource hardening so repeated failing page queries cannot retain Chrome-side
handles until context teardown.

## Focused verification

The branch's recorded real-browser, scorecard, and resource results were
reused. Independent inexpensive checks passed:

- `cargo test browser::session::tests --lib` — 16 passed;
- `cargo test browser::cdp::tests --lib` — 8 passed;
- `git diff --check 039ffbb 90c1a0f`.

No implementation changes were made during this audit.
