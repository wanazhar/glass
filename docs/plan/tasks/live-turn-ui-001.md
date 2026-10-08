---
id: live-turn-ui-001
scope: live terminal drawing, global viewer keys, expanded tools and provider reasoning view
status: in-progress
depends-on: []
---

## Objective

Implement ../live-turn-ui.md, addressing the user's screenshots and requested expanded Bash commands/live provider thinking view.

## Paths

Main renderer/pump/viewers; glass-tui editor/transcript/shared view helpers; agent Progress; provider event decoders and session presentation; PTY scripts, docs, CI as needed.

## Integration

Provider events -> agent Progress -> live view state -> owned main or overlay surface. Keyboard pump -> global active Ctrl-G/Ctrl-T and draft -> modal state without stopping work. Bash command/progress -> expanded tool cards -> exact original details/logs.

## Verification

Workspace tests, Clippy, all seven PTY suites plus dedicated live-turn drawing/reasoning suite using accurate VT projection, independent review. Stable Rust source before root installs; verifier runs installed binary. No commits or remote publication.

## Current gate

Implementation and offline release build are complete. Round-two workspace tests (381 passed) and Clippy passed; six of nine shipping PTY suites and independent five-size live/navigation plus native tmux resize/shell restoration passed. Latest onboarding, polish and chat fixture updates remain unverified: the environment now denies localhost listeners. Review is explicitly blocked in ../reviews/live-turn-ui-001-01.md.

The supported installer succeeded with a private root at `/tmp/gc-final-install`; its `gc` is a relative symlink to `glass-core`. Default `~/.cargo/bin/gc` cannot be updated under the current write restrictions. Full provider-test reruns and default installation remain outstanding. No publication occurred.
