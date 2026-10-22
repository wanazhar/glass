# Review: live-turn-ui-001

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Status: **VERIFICATION BLOCKED** (October 7, 2026)

## Scope reviewed

Reviewed the task and design contract, then inspected the managed terminal surface and live-view implementation. The source has a single app-lifetime alternate screen, a bounded rendered-body cache, a temporary spool removed on drop, and shared transcript/reasoning overlays. Provider reasoning is sanitized for display and the live view has an explicit empty state. Independent tests previously exercised exact answer records, live reasoning, busy transcript toggles, draft retention and resize.

## Evidence

- `cargo test --workspace --locked`: round-two source passed 381 tests (305 `glass-core`, 11 `glass-tui`, 65 `glass-mod-api`); one doctest was ignored. Log: `/tmp/gc-final-tests-r2.log`.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed. Log: `/tmp/gc-final-clippy-r2.log`.
- Round-two PTY suites passed: features, workflows, mobile, motion, live and navigation. The live suite used five sizes. Logs: `/tmp/gc-final-r2-{features,workflows,mobile,motion,live,navigation}.log`.
- Independent live and navigation PTY reviews passed at five sizes: `/tmp/gc-final-independent-live.log`, `/tmp/gc-final-independent-navigation.log`.
- Native `tmux` alternate-screen and primary-resize checks passed, including shell sentinel restoration: `/tmp/gc-final-tmux-live.log`, `/tmp/gc-final-tmux-primary.log`.
- After loopback became unavailable, an offline PTY check passed at 40×24 and 20×8: startup, Ctrl-P palette cancel with draft restoration, `/keys` scroll/Esc, one alternate-screen entry, clean exit and shell sentinels before/after the app. It made no provider requests.

## Outstanding verification

Onboarding, polish and chat did not pass the round-two PTY batch. Subsequent runs exposed PTY `EIO` cleanup in onboarding, a polish attachment assertion that may have consumed stale response output, and a chat resize assertion based on a legacy ANSI projection. The fixture owner updated response matching and the chat VT projection, but loopback mock-provider startup is now prohibited in this environment (`PermissionError: Operation not permitted`), so those corrections could not be verified.

A release build and private install to `/tmp/gc-final-install` subsequently passed. The `gc` alias resolves to `/tmp/gc-final-install/bin/glass-core` and reports `glass-core 0.1.0`. Offline PTY checks passed at 40×24 and 20×8 for startup, palette cancel with draft restoration, `/keys` scrolling/close, `/security`, one alternate-screen entry, clean exit and shell sentinel restoration. The headless JSON missing-key path returned clean structured output without a provider request. Logs: `/tmp/gc-final-private-install.log` and `/tmp/gc-final-private-offline-pty.log`.

The private install has offline coverage only. No completion claim is made until the three provider-backed suites pass against it in an environment that permits the mock loopback server.
