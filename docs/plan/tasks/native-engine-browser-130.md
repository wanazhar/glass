---
id: native-engine-browser-130
scope: glass-browser/native-engine/text-selection
status: completed
depends-on: [native-engine-browser-129]
---

# BE-44: native text selection and caret ownership

## Objective

Make the native keyboard path useful for real editing by giving focused
`input` and `textarea` controls an engine-owned selection and caret. A native
shortcut must be able to select a range, move or extend the caret, replace the
range, and delete it in both local and HTTP(S) content-process documents.

## Contract

- Text controls retain bounded start/end character offsets in the Rust DOM
  owner and transfer them through the content-process document wire.
- First focus places a new text-control caret at the value end; later focus
  preserves the range. `type`, clear, script value assignment, and editing
  defaults update the range deterministically.
- The supported default actions are Ctrl/Meta+A, left/right/Home/End with
  optional Shift extension, printable insertion, Backspace, and Delete. A
  range is replaced or deleted atomically and emits one input effect only when
  the value changes.
- The JavaScript host exposes bounded `selectionStart`, `selectionEnd`,
  `selectionDirection`, `setSelectionRange`, and `select` state/commands. Page
  keydown cancellation still suppresses the default action.
- Local and HTTP(S) content-process documents use the same default-action and
  revision transaction. Chromium keeps its existing keyboard adapter; other
  partial adapters reject the shared action variants explicitly.

## Deliberate boundary and tradeoffs

Offsets are Unicode scalar positions rather than platform text-service or
UTF-16 storage offsets, matching the existing bounded Rust editing model and
avoiding byte-index corruption. This is deterministic and sufficient for the
semantic editor contract, but browser-grade grapheme clusters, bidi caret
geometry, clipboard, IME/composition, dead keys, accessibility selection
events, and OS key repeat remain separate issue #40 work. Selection is kept in
the document owner so a child cannot publish an unvalidated range.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
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
itself. Grapheme-cluster and bidi caret geometry, clipboard, IME/composition,
dead keys, accessibility selection events, and OS key repeat remain later
issue #40 work.
