id: native-engine-browser-736
scope: glass-dev/browser-service-native-modal-dialog-control
status: done
depends-on: [native-engine-browser-735]

# Glass native-engine browser slice 736: resident service dialog control

## Objective

Give the resident native `BrowserService` an explicit modal-dialog mode and a
thread-safe out-of-band controller. A host must be able to inspect and resolve
the exact pending dialog while the service worker is suspended in navigation
or script execution. The existing default remains non-modal until a responsive
host surface opts in. Chromium attach sessions must reject native modal mode.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-735.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `crates/glass-dev/src/browser.rs`

## Contract

- `BrowserStartConfig.modal_dialogs` is opt-in and defaults to `false`.
- Enabling it starts the native runtime with `connect_native_with_modal_dialogs`
  and retains a cloneable `NativeDialogController` outside the serialized
  browser command queue.
- `BrowserService` exposes exact pending-dialog lookup and resolution methods
  that remain usable while a browser command is blocked on the page dialog.
- A stale ID is rejected without resuming the page script; a valid response
  resumes the same operation once.
- Closing/restarting the service clears the old controller before disposing
  the session. Non-modal and Chromium sessions fail explicitly when a caller
  requests modal control.
- This slice does not add TUI presentation or change the default service mode.

## Path

- `crates/glass-dev/src/browser.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-736.md`

## Verification

- `cargo check -p glass-dev --lib --tests --locked --quiet` passed.
- `cargo test -p glass-dev --lib native_dialog --locked -- --nocapture`
  passed (2 tests). The local-HTTP integration test suspended navigation at
  `confirm` and `prompt`, rejected a stale ID without releasing the script,
  resolved both exact IDs, and verified the original navigation completed.
- The first default-stack integration run exposed stack exhaustion on the
  `glass-browser-workspace` worker. An 8 MiB stack for that worker fixed it;
  the focused tests then passed without an environment override.
- `rustfmt --edition 2024 --check crates/glass-dev/src/browser.rs` and
  `git diff --check` passed.
- TUI/CLI/MCP dialog presentation, default modal mode, in-process realms,
  `beforeunload` dialogs, cross-platform certification, remote CI, release, and
  browser completion remain open.
