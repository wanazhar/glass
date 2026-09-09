---
id: native-engine-browser-033
scope: glass-browser/native-engine/keyboard-input
status: done
depends-on: [native-engine-browser-032]
---

# BE-03j/BE-04o: bounded native semantic keyboard input

## Objective

Route the stable `SemanticAction::KeyPress` contract through the native
backend so a focused text control can receive bounded keyboard edits in both
the local owner and sandboxed content process.

## Contract

- `KeyPress` accepts printable single-scalar keys plus `Backspace` and
  `Delete`, with a 64-byte key bound and the existing text-value quota.
- The action targets the current focused `input` or `textarea` textbox. The
  bounded editing model appends printable keys and removes the final scalar
  for `Backspace`; `Delete` is a no-op at the end-of-value caret boundary.
- Local and child-owned actions dispatch cancelable `keydown` with `key` and
  `code`, apply the default edit only when it was not canceled, dispatch
  `input` when the value changed, and always dispatch `keyup`.
- Persistent JavaScript element wrappers are refreshed from the committed Rust
  snapshot before event callbacks, so captured element references observe the
  current value instead of a stale evaluation snapshot.
- The parent commits one revision and one bounded privacy-safe effect batch;
  raw key strings and text values do not enter effect metadata or IPC event
  records beyond the typed request boundary.

## Deliberate boundary and tradeoffs

- Selection and caret ranges are not modeled; printable edits use the bounded
  end-of-value caret and `Delete` therefore has no effect.
- `Enter`, `Tab`, arrows, Home/End, modifiers/shortcuts, repeat, keyboard
  navigation, `beforeinput`, composition/IME, clipboard, and form-submit
  default actions remain open. Supporting them requires selection state,
  richer keyboard event metadata, and additional focus/form lifecycle rules.
- This does not claim full live Web IDL identity. Wrapper refresh makes the
  current snapshot observable to retained callbacks while DOM insertion,
  removal, and general identity semantics remain bounded/open.

## Paths

- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_keypress_edits_focused_text_and_honors_keydown_cancel -- --nocapture` — 1/1 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_owns_external_form_mutations_and_effects -- --nocapture` — 1/1 passed, including child key bridge.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine semantic_actions_and_effects_use_the_backend_contract -- --nocapture` — 1/1 passed.
- `git diff --check`

The next browser-complete gates remain parser-blocking/defer/async ordering,
selection and richer keyboard/input lifecycle, full form submission/default
actions, Fetch/XHR, browsing contexts, and the remaining Web IDL/resource
surface.
