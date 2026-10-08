# Review: inline-scroll-001

Status: **PARTIALLY VERIFIED — PROVIDER-BACKED UPDATE/CANCEL GATE BLOCKED** (October 7, 2026)

## Independent offline PTY review

- `/tmp/gc-inline-review.py` against `target/debug/glass-core` passed at 65×30, 40×24, 24×16, 20×12 and 20×8. It used a private synthetic 64-turn JSONL session and made no provider requests.
- At each size, the first PageUp opened near the newest content in paused mode, PageUp moved the view, the editor stayed pinned, the insertion cursor returned to its original cell, and bracketed paste changed the draft without moving the reader position.
- Ctrl-P opened the command palette over the reader; Esc restored the same reader line and the pasted draft. PageDown to the tail and SGR wheel-down restored the `following` state. SGR wheel-up/down worked while paused.
- The unchanged idle view emitted no output during a 0.55 second interval. An idle resize from 40×24 to 20×8 and back to 40×24 redrew without a keypress, retained the draft, and emitted no further bytes for 0.5 seconds after geometry settled.
- Ctrl-G transcript and Ctrl-T reasoning views both closed back to the owned main body with the draft visible. A single alternate screen was used. Exiting after reader use restored the alternate screen and both mouse-capture modes; process status was zero.
- The independent page-position check allows part of the newest turn to remain visible on a narrow viewport; it verifies the reader is paused near the tail rather than requiring an entire long turn to fit.

## Focused code gates

- `cargo test -p glass-tui --locked transcript::tests -- --nocapture`: 18 passed.
- `cargo test -p glass-core --locked surface::tests -- --nocapture`: 4 passed.
- `cargo clippy -p glass-tui -p glass-core --all-targets --locked -- -D warnings`: passed.
- Logs: `/tmp/gc-inline-independent-review.log`, `/tmp/gc-inline-switch-review.log`, `/tmp/gc-inline-exit-review.log`, `/tmp/gc-inline-transcript-tests.log`, `/tmp/gc-inline-surface-tests.log` and `/tmp/gc-inline-clippy.log`.
- The same three independent PTY suites also passed against the private release install at `/tmp/gc-final-install/bin/gc`: `/tmp/gc-inline-installed-review.log`, `/tmp/gc-inline-installed-switch.log` and `/tmp/gc-inline-installed-exit.log`. The install log is `/tmp/gc-inline-private-install-final.log`. The existing default `glass`/`glass-core` command was not replaced.

## Still outstanding

- The sandbox denies loopback listener creation, so active-provider updates while paused, unread-count changes as new streamed content arrives, and cancellation during a live provider/tool turn remain unverified. Offline synthetic history does not substitute for that gate.
- A standalone crossterm probe in this runner did not receive SIGWINCH from the PTY fixture (its reported foreground process group was zero). The app's idle geometry polling fallback independently passed the no-key resize checks above.

No claim is made about live-provider update, unread, or cancellation behavior.
