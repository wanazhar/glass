id: issue64-three-thread-journey-001
scope: issue-64-concurrent-project-thread-journey
status: complete
depends-on: [issue64-project-thread-picker-001]
---

# Three-thread homespace journey

## Objective

Combine concurrent fake-Core work, project-thread switching, per-thread draft
and cursor restoration, and focus-safe background completion in one allocated
PTY journey. Keep each prompt attempt, draft, and cursor attached to its owning
conversation while a different conversation remains selected.

## Context

- [Glass documentation index](../../INDEX.md)
- [Development TUI architecture](../../architecture/development-tui.md)
- [Project-local thread picker](issue64-project-thread-picker-001.md)
- [Glass delivery plan](../README.md)

## Paths

- `crates/glass-dev/tests/tui_pty.rs`
- `docs/plan/README.md`

## Acceptance

- [x] One fake-Core PTY session starts three separate resident threads and
      holds all three turns active together.
- [x] Each thread has a distinct composer draft and non-terminal character
      cursor; switching between threads restores the owning text and cursor.
- [x] Alpha completes in the background while gamma remains selected. Gamma's
      focus, draft, and cursor remain unchanged, and alpha stays independently
      selectable with its settled attempt.
- [x] The persisted registry keeps each attempt, status, draft, and cursor
      associated with the matching thread; beta and gamma remain active after
      alpha completes and can settle afterward.
- [x] The allocated PTY journey, strict scoped Clippy, formatting, and
      documentation validators pass. Evidence names the exact binary and
      distinguishes it from a fresh release build.

## Verification

Run the allocated PTY journey with `GLASS_E2E=1` and an explicit
`GLASS_E2E_BINARY`. Run `cargo clippy -p glass-dev --test tui_pty --locked --
-D warnings`, `cargo fmt --all -- --check`, documentation coverage/depth/
release-truth/TUI-shortcut checks, Python fixture syntax validation, and
`git diff --check`.

## Evidence

The combined fake-Core PTY journey passed against the pre-existing debug binary:

```text
CARGO_TARGET_DIR=/home/ubuntu/work/glass/target GLASS_E2E=1 \
  GLASS_E2E_BINARY=/home/ubuntu/work/glass/target/debug/glass \
  cargo test -p glass-dev --test tui_pty \
  three_fake_core_tasks_complete_without_stealing_selected_thread \
  --locked -- --nocapture
1 passed; 0 failed
```

The test starts alpha, beta, and gamma prompts behind the fixture's three
independent gates, then persists a distinct draft and cursor in each thread.
It releases alpha while gamma remains selected and checks the picker still
highlights gamma. After the picker closes, it captures a fresh conversation
frame and verifies gamma's prompt is displayed without alpha's response. It
waits for the registry to record alpha as settled while beta and gamma remain
working under their own prompt text and target RunIds. It then visits alpha,
beta, and gamma; inserting and removing a character at each restored cursor
confirms the live composer position as well as the saved text/cursor ownership.
Beta and gamma are released afterward and all three attempts settle in their
owning records.

This used `/home/ubuntu/work/glass/target/debug/glass`, the existing local
`glass 0.3.14` debug binary. No release binary was built. The existing fake-Core
fixture already provides the isolated alpha/beta/gamma gates; this task changed
neither fixture nor product source.

Validation passed: strict `cargo clippy -p glass-dev --test tui_pty --locked --
-D warnings`, `cargo fmt --all -- --check`, Python fixture syntax validation,
documentation coverage (using the existing root debug binaries), documentation
depth, release-truth, TUI shortcut inventory, and `git diff --check`.

Independent review initially found that a picker-frame snapshot was being
reused to claim conversation isolation. The remediation waits for the alpha
ledger settlement and captures gamma's conversation after closing the picker.
Re-review passed at commit `785212a6` with no remaining findings.

## TUI responsiveness baseline

On 2026-10-23, a small Linux baseline used the existing `target/debug/glass`
binary, the offline mixed-mode fake Core fixture, an isolated temporary project
and data directory, and an allocated 80×24 PTY. Three launches measured from
process start until the initial `Workspace is untrusted` frame reached the PTY:
388 ms, 384 ms, and 386 ms. With the TUI settled, pressing `?` produced the
help overlay in 2 ms in each of three samples. During a five-second idle window
after startup, sampled process-tree CPU averaged 2.8%, 2.8%, and 3.0%; peak
aggregate resident memory was 44.8, 45.6, and 45.6 MiB. The process-tree totals
include the fake Core fixture and its Python runtime. Measurements were read
from `/proc` and the PTY; the first-frame marker is a terminal write, not a
physical display or compositor measurement. This is a local debug-build
baseline, not a release-build, cross-platform, or statistically stable
benchmark.

For subsequent Linux debug-build comparisons on the same fixture and terminal,
use provisional regression budgets of 1,000 ms to the initial trust frame,
100 ms from the help key to visible overlay output, 5% average process-tree CPU
over a settled five-second idle window, and 64 MiB peak process-tree RSS. These
are deliberately rounded above this small sample and are not product-wide
performance claims. The three-thread PTY journey above provides a separate
background-update behavior check: alpha settles while gamma remains selected,
and gamma's focus, draft, and cursor stay unchanged. That test verifies the
observable behavior, not the latency of background updates under a sustained
event load.
