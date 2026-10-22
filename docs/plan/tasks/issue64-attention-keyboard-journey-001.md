---
id: issue64-attention-keyboard-journey-001
scope: issue-64-keyboard-attention-review-and-exit
status: complete
depends-on: [issue63-attention-inbox-001, issue64-project-thread-picker-001]
---

# Attention and review keyboard journey

## Objective

Verify that an operator can inspect an uncertain delivery from the shared
Attention Inbox, navigate to a pending editor proposal, and cancel then confirm
application exit using the keyboard alone.

## Context

- [Glass documentation index](../../INDEX.md)
- [Development TUI architecture](../../architecture/development-tui.md)
- [Attention inbox](issue63-attention-inbox-001.md)
- [Project-local thread picker](issue64-project-thread-picker-001.md)
- [Glass delivery plan](../README.md)

## Paths

- `crates/glass-dev/tests/tui_pty.rs`
- `docs/plan/README.md`

## Acceptance

- [x] At 80×24 and 50×16, a seeded uncertain delivery and editor proposal
      appear together in the Attention Inbox.
- [x] Activating the uncertain item opens its owning conversation with the
      saved message and keeps the attempt uncertain without replay.
- [x] The unrelated proposal remains available after inspecting the uncertain
      delivery.
- [x] Activating the remaining proposal opens its exact file review context.
- [x] Ctrl-C opens the review editor's unsaved-exit prompt; Escape keeps the
      review active, and a later Ctrl-C followed by `Q` opens the application
      quit confirmation, which Enter confirms before exiting.
- [x] Focused PTY, scoped strict Clippy, formatting, docs
      validators, and diff checks pass; evidence identifies the exact binary
      used and does not imply a fresh release build unless one was built.

## Verification

Run the allocated PTY journey with `GLASS_E2E=1` and an explicitly named
`GLASS_E2E_BINARY`. Run `cargo clippy -p glass-dev --test tui_pty --locked --
-D warnings`, `cargo fmt --all -- --check`, documentation
coverage/depth/release-truth/shortcut checks, and `git diff
--check`.

## Evidence

The combined offline PTY journey passed at both 80×24 and 50×16:

```text
GLASS_E2E=1 GLASS_E2E_BINARY=/home/ubuntu/work/glass/target/debug/glass \
  cargo test -p glass-dev --test tui_pty \
  attention_review_keyboard_journey_preserves_uncertain_send_and_exits_cleanly \
  --locked -- --nocapture
1 passed; 0 failed
```

The test used the existing local `glass 0.3.14` debug binary at the explicit
path above; this task did not build a release binary. It verifies both inbox
items together, uncertain-attempt activation without `turn/prompt` replay,
proposal review on `a-first.txt`, Escape cancellation of the editor exit prompt,
and the final Enter-confirmed discard-and-quit prompt.

Scoped validation passed: `cargo clippy -p glass-dev --test tui_pty --locked --
-D warnings`, `cargo fmt --all -- --check`, Python fixture syntax validation,
documentation coverage/depth/release-truth/TUI-shortcut validators, and
`git diff --check`.

Independent review passed with no findings: [review record](../reviews/issue64-attention-keyboard-journey-001-01.md).
