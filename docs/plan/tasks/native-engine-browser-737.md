id: native-engine-browser-737
scope: glass-dev/tui-native-browser-dialog-presentation
status: done
depends-on: [native-engine-browser-736]

# Glass native-engine browser slice 737: TUI modal-dialog presentation

## Objective

Connect the resident native browser's out-of-band dialog controller to the
Glass Dev App surface. TUI-started native sessions opt into modal dialogs; the
TUI polls and resolves the controller without taking the workspace lock or
waiting for the suspended browser tool. Present responsive, focused controls
for alert, confirm, and prompt, and release a pending dialog if the user
confirms quitting Glass.

## Context

- `docs/INDEX.md`
- `docs/architecture/development-tui.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-736.md`
- `crates/glass-dev/src/browser.rs`
- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `crates/glass-dev/src/tui/render.rs`
- `crates/glass-dev/src/tui/command.rs`

## Contract

- Browser starts initiated by the Glass Dev TUI pass `modalDialogs: true`;
  ordinary `BrowserService` and standalone Rust defaults remain unchanged.
- `DevTuiState` retains a clone of the thread-safe `BrowserService` handle so
  dialog status and resolution do not acquire the shared workspace lock.
- While a native modal is pending, the page script and navigation remain
  suspended, but the TUI event loop remains responsive and exclusive dialog
  input is routed ahead of ordinary surface/composer/palette keys.
- Alert Enter/Esc acknowledges; confirm Y/Enter accepts and N/Esc dismisses;
  prompt Enter submits edited text and Esc dismisses. Prompt input begins with
  the page-provided default. Resolution uses the exact pending dialog ID.
- A stale resolution remains visible without applying to a newer dialog. New
  identities reset prompt input; repeated polls preserve edits. External close
  removes the overlay. Resize/focus changes do not dismiss it. Confirmed TUI
  quit dismisses any open modal before worker/session cleanup.
- The centered overlay adapts to desktop/compact/phone widths and suppresses
  live Kitty pixels beneath it.

## Path

- `crates/glass-browser/src/browser.rs`
- `crates/glass-browser/src/browser/native_engine/dialog.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/src/browser/mod.rs`
- `crates/glass-browser/src/lib.rs`
- `crates/glass-dev/src/browser.rs`
- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `crates/glass-dev/src/tui/render.rs`
- `crates/glass-dev/src/tui/command.rs`
- `docs/architecture/development-tui.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-737.md`

## Verification

- Focused reducer/key tests for alert, confirm, prompt editing, exact-ID
  failures, and quit cleanup.
- Ratatui render assertions at desktop and phone sizes.
- Process-backed BrowserService integration remains the execution boundary;
  TUI tests verify that its retained handle sees/resolves without the workspace
  lock.
- Run the scoped `glass-dev` check and affected tests once after the behavior
  batch, plus formatting, docs gates, and a terminal capture/screenshot.
