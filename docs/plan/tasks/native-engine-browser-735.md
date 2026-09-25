id: native-engine-browser-735
scope: glass-browser/standalone-native-modal-dialog-controller
status: complete
depends-on: [native-engine-browser-734]
---

# Glass native-engine browser slice 735: standalone modal-dialog controller

## Objective

Expose the already-implemented process-backed modal-dialog rendezvous through
an explicit, typed Rust-session API so an embedding application can inspect
and resolve a page dialog while the original navigation or evaluation future
is suspended. The controller must not acquire the session's serialized
page-operation lock, and every resolution must match the current dialog ID.

The modal behavior remains opt-in. Ordinary native session constructors keep
the nonblocking dialog-event path until a CLI/MCP/TUI host has a responsive
controller. This task does not implement UI presentation or claim full
cross-surface dialog parity.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-734.md`
- `docs/rust-sdk.md`
- `docs/features.md`
- `crates/glass-browser/src/browser/native_engine/dialog.rs`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/browser/native_backend.rs`

## Contract

- `BrowserRuntimeSession::connect_native_with_modal_dialogs` (also available
  through the canonical `BrowserSession` alias) explicitly enables the
  process-backed blocking dialog host callback. Existing `start`,
  `start_default`, and `connect_native` behavior remains nonblocking.
- `native_dialog_controller()` returns a cloneable controller only for a
  modal-enabled native session. It returns an explicit error for an ordinary
  native session with modal control disabled or for a non-native runtime.
- `pending_dialog()` returns the exact pending ID, owning context/target,
  frame, and bounded dialog metadata, or `None` when no modal is open. It is
  out-of-band and does not wait on or acquire the page-operation lock.
- `resolve_dialog(id, resolution)` accepts or dismisses only the matching
  active ID. Prompt response text is accepted only for an accepted prompt and
  remains subject to the existing byte limit. Invalid, stale, duplicate, or
  mismatched responses preserve the pending dialog and do not resume script.
- The script remains suspended at the original `alert`, `confirm`, or
  `prompt` call and resumes once with the HTML dialog result after resolution.
  Navigation, worker failure, or session shutdown cannot apply a late answer
  to another document.
- The controller introduces no CDP, remote-browser, or implicit fallback.
  In-process engine realms and user-facing TUI presentation remain separate
  issue #40 gates.

## Integration path

1. The explicit session constructor installs the existing modal-enabled
   `NativeDialogControlPlane` in the process-backed native backend.
2. The session returns a typed cloneable controller over that same control
   plane; it does not expose the internal worker channel or child dialog ID.
3. A caller runs a browser operation concurrently, reads the pending prompt
   through the controller, and resolves its exact ID. The original operation
   completes only after the content process resumes.

## Path

- `crates/glass-browser/src/browser/native_engine/dialog.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/src/browser/mod.rs`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/lib.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/architecture/native-engine.md`
- `docs/rust-sdk.md`
- `docs/features.md`
- `docs/ownership.md`
- `docs/plan/tasks/native-engine-browser-735.md`

## Verification

- Unit-test controller pending lookup, exact-ID enforcement, prompt response
  validation, duplicate resolution, and closed/abandoned-dialog behavior.
- Add a process-backed HTTP integration test using the public session
  constructor: suspend a navigation in a JavaScript dialog, inspect the exact
  target/frame-owned prompt through the public controller, reject a stale ID
  without releasing the script, resolve the correct ID, and prove the original
  script continues once with the selected result.
- Prove the ordinary native constructor rejects acquisition of the modal
  controller and retains its existing nonblocking dialog-event behavior.
- `cargo check --locked --quiet -p glass-browser --lib --test native_engine`
  passed.
- `cargo check --locked --quiet -p glass-browser --lib --tests` passed.
- `cargo test --locked -p glass-browser --lib public_controller_ --
  --nocapture` passed (2 tests): stale/duplicate identities preserve exact
  resolution and a non-modal control plane cannot expose the public controller.
- `cargo test --locked -p glass-browser --test native_engine
  standalone_native_dialog_controller_resumes_the_original_page_script --
  --nocapture` passed (1 test, 20.35s; the integration test target compiled in
  2m58s). It rejects a stale ID without releasing the script, resolves confirm,
  prompt, and alert through the public controller, and verifies the same page
  script completes once with the selected values.
- `cargo test --locked -p glass-browser --test native_engine
  native_dialogs_are_owned_by_the_page_realm_and_prompt_backend -- --nocapture`
  passed (1 test), preserving nonblocking behavior for ordinary native
  constructors.
- Rust formatting, whitespace, release-truth, documentation-depth, and
  shortcut checks passed. Documentation coverage was not run because it needs
  the unrelated `target/debug/glass` binary; `glass-dev` was not built merely
  for that verifier.
- Keep CLI/MCP/TUI routing, human-facing dialog UI, in-process realms,
  `beforeunload`, cross-platform certification, remote CI, release, and
  browser-completion claims explicitly open.
