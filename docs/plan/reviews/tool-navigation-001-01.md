# Review: tool-navigation-001

Status: **VERIFICATION BLOCKED** (October 7, 2026)

## Scope reviewed

Reviewed the searchable tool inspector and `/keys` implementation against the task contract. The independent navigation test covers searching commands and recorded output, empty results, explicit exact command/output copies, filtered terminal-control previews, no command execution, keyboard discovery and five terminal sizes.

## Evidence

- The round-two navigation PTY suite passed at all five requested sizes, including exact OSC 52 clipboard payload checks and keyboard discovery: `/tmp/gc-final-r2-navigation.log`.
- Independent navigation PTY review passed at five sizes: `/tmp/gc-final-independent-navigation.log`.
- Workspace tests and Clippy passed on the same round-two source; details are recorded in [the live-turn UI review](live-turn-ui-001-01.md).
- Earlier navigation testing caught a managed-screen ownership regression in the command palette. The source was corrected to use the app-managed screen, and the passing round-two suite verified a single alternate-screen entry across palette and viewer flows.
- An offline PTY check independently passed command-palette cancel/draft restore and `/keys` scrolling/close at 40×24 and 20×8, with one alternate-screen entry and shell restoration. It does not exercise recorded tool inspection or clipboard copying.

## Outstanding verification

A release build and private install to `/tmp/gc-final-install` passed. `/tmp/gc-final-install/bin/gc` resolves to `/tmp/gc-final-install/bin/glass-core` and reports `glass-core 0.1.0`. Offline checks covered startup, palette cancel/draft restoration, `/keys` scrolling/close, `/security`, one alternate-screen entry, shell restoration, clean exit, help and structured headless missing-key output. Logs: `/tmp/gc-final-private-install.log` and `/tmp/gc-final-private-offline-pty.log`.

The updated provider-backed navigation suite could not be rerun after loopback socket creation began failing with `PermissionError: Operation not permitted`. The round-two five-size PTY and independent navigation passes are evidence for that source snapshot; they do not verify the private-installed binary's recorded-tool search or clipboard path. No completion claim is made until provider-backed navigation passes against the private install in an environment permitting the mock loopback server.
