---
id: issue64-attention-keyboard-journey-001
scope: issue-64-keyboard-attention-review-and-exit
status: in-progress
depends-on: [issue63-attention-inbox-001, issue64-project-thread-picker-001]
---

# Attention and review keyboard journey

## Objective

Verify that an operator can inspect an uncertain delivery from the shared
Attention Inbox, cancel its explicit retry confirmation without resending,
continue to a pending editor proposal, and cancel then confirm application exit
using the keyboard alone.

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

- [ ] At 80×24 and 50×16, a seeded uncertain delivery and editor proposal
      appear together in the Attention Inbox.
- [ ] Activating the uncertain item opens its owning conversation with the
      saved message; requesting retry shows the duplicate-risk confirmation.
- [ ] Escape cancels that confirmation, leaves the attempt uncertain, and sends
      no `turn/prompt`; the unrelated proposal remains available.
- [ ] Activating the remaining proposal opens its exact file review context.
- [ ] Ctrl-C opens quit confirmation from the review context; Escape cancels,
      and a later Ctrl-C plus confirmation exits cleanly.
- [ ] Fixture and TUI child processes have bounded cleanup on assertion failure.
- [ ] Focused PTY, scoped strict Clippy, formatting, fixture syntax, docs
      validators, and diff checks pass; evidence identifies the exact binary
      used and does not imply a fresh release build unless one was built.

## Verification

Run the allocated PTY journey with `GLASS_E2E=1` and an explicitly named
`GLASS_E2E_BINARY`. Run `cargo clippy -p glass-dev --test tui_pty --locked --
-D warnings`, `cargo fmt --all -- --check`, fixture syntax validation,
documentation coverage/depth/release-truth/shortcut checks, and `git diff
--check`.

## Evidence

Pending implementation and review.
