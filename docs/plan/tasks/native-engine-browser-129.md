---
id: native-engine-browser-129
scope: glass-browser/native-engine/keyboard-actions
status: completed
depends-on: [native-engine-browser-128]
---

# BE-43: native keyboard action ownership

## Objective

Move the normal Glass keyboard vocabulary through the semantic backend and
native engine. Native callers must be able to dispatch an explicit key-down,
key-up, or complete shortcut sequence to the focused page control without
opening a CDP session.

## Contract

- `KeyDown` and `KeyUp` validate one printable bounded key and dispatch the
  corresponding cancelable/non-cancelable DOM event to the focused actionable
  page target.
- `KeyPress` retains its existing default-edit behavior and is represented as
  a key-down/default action/key-up sequence.
- `Shortcut` validates one or more modifiers plus exactly one non-modifier key,
  dispatches the modifier-aware key sequence in deterministic order, and
  applies the bounded default edit only when the key is an editable printable
  key and the event is not canceled.
- Local documents and process-backed HTTP(S) documents use the same event and
  revision transaction. A rejected or canceled event cannot publish a partial
  document snapshot.
- Chromium maps the expanded semantic actions to the existing
  `BrowserSession` keyboard methods. Other adapters reject actions they do not
  certify; native dispatch never falls back to CDP.
- Native one-shot CLI dispatch maps `key-down`, `key-up`, and `shortcut` and
  preserves revision guards as an explicit future integration gate until the
  portable session carries them.

## Deliberate boundary and tradeoffs

The action carries semantic key intent rather than OS scancodes. This keeps
the backend portable and lets the native DOM/event owner enforce focus,
disabledness, cancellation, and bounded default actions. Modifier state is
ephemeral to one shortcut transaction; persistent keyboard layout, IME
composition, selection ranges, OS key repeat, and platform text services remain
separate browser-primitive work. Adding variants to the shared enum forces
every adapter to make an explicit support decision at compile time.

## Paths

- `crates/glass-browser/src/browser_backend.rs`
- `crates/glass-browser/src/browser/backend_adapter.rs`
- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/bidi_backend.rs`
- `crates/glass-browser/src/browser/webdriver_backend.rs`
- `crates/glass-browser/src/browser/proof_backend.rs`
- `crates/glass-browser/src/cli/runner.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine --lib`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_keypress_edits_focused_text_and_honors_keydown_cancel -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_keyboard_actions_preserve_event_and_modifier_contract -- --nocapture`
- Both focused tests passed; the native feature library check passed.

This task does not promote native certification or claim full CDP replacement by
itself. Persistent selection/caret state, IME and text services, repeat,
composition, and the remaining browser keyboard contract remain later issue
#40 work.
