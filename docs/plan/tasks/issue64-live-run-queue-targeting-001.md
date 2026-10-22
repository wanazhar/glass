---
id: issue64-live-run-queue-targeting-001
scope: issue-64-live-follow-up-queue-targeting
status: complete
depends-on: [issue63-durable-followup-queue-001, issue64-send-attempt-recovery-001]
---

# Live RunId follow-up queue targeting

## Objective

Prove through the real TUI and offline Core runtime that a queued steer remains
bound to its recorded active `RunId`, reaches that run while its prompt is
active, and records acceptance and settlement from the corresponding runtime
responses.

## Context

- [Durable per-thread follow-up queue](issue63-durable-followup-queue-001.md)
- [Durable send attempt recovery](issue64-send-attempt-recovery-001.md)
- [Development Runtime](../../development-runtime.md)

## Paths

- `crates/glass-dev/tests/fixtures/fake_glass_core_runtime.py`
- `crates/glass-dev/tests/tui_pty.rs`

## Acceptance

- [x] An allocated PTY starts a fake-Core run and observes its stable active
      RunId while the fake runtime holds the initial prompt open.
- [x] A steer queue item retains that RunId and is delivered as `turn/steer`
      while the original prompt is still active.
- [x] A positive Core acknowledgement changes the queue row to accepted, and
      correlated runtime responses settle the attempts without relying on a
      run-level settled event alone.
- [x] The gated fixture stays offline and has bounded child-process cleanup.
- [x] Focused PTY, formatting, strict scoped Clippy, relevant documentation
      validators, and diff review pass.

## Evidence

`active_run_queue_steer_reaches_its_run_and_settles_truthfully` runs the real
Glass TUI in an allocated PTY with the offline fixture runtime. It waits for
the prompt attempt to be recorded as working, reads its RunId from the durable
thread ledger, then adds a steer row and checks that the row retains that exact
RunId. The fixture gate keeps the original prompt open while it reads and
acknowledges `turn/steer`; the PTY checks the runtime session, text, and
`context.targetRunId`. Before releasing the initial prompt, the test verifies
that the steer attempt is settled while the original prompt attempt remains
working. It then releases the prompt and verifies both attempts settle. This
exercises request-ID response correlation separately from the later run-level
settled event.

The fixture waits only when this test's `.glass/e2e-core-active-steer` marker
exists and the prompt contains its dedicated test token. The PTY guard kills
the exact fixture PID on unwinding, and `Session` cleanup waits for the TUI
child. An earlier combined run displayed `Could not verify the current run
destination` after the queue write. Its ledger still showed a ready row and
only the initial prompt attempt, and its request log had no steer call. The
journey retries Enter only after checking that same ready/no-attempt/no-request
state, so an accepted delivery is never repeated.

With `GLASS_E2E_BINARY=/home/ubuntu/work/glass/target/debug/glass`, the focused
queue PTYs passed: 2 tests, 0 failures. The fixture syntax check, workspace
format check, `git diff --check`, `cargo clippy -p glass-dev --test tui_pty
--locked -- -D warnings`, release-truth, documentation depth/coverage, and TUI
shortcut validators also passed. The PTY used the existing debug Glass binary;
no provider account or network was involved.
