---
id: native-engine-browser-021
scope: glass-browser/native-engine/javascript-rust-action-bridge
status: done
depends-on: [native-engine-browser-020]
---

# BE-03e/BE-04f: Rust semantic-action event bridge

## Objective

Route Rust-owned semantic action effects back into the page realm without
moving executable callbacks across the native IPC boundary. Local actions must
use the persistent owner-side realm; external actions must dispatch inside the
sandboxed content process and transfer only the resulting typed mutation.

## Contract

- After a committed Rust semantic action, the engine converts bounded
  privacy-safe effect metadata into an internal host-event script containing
  only node indices, event type, bubbles, and cancelable flags.
- The local engine executes that script in its existing JavaScript realm. The
  child process executes the same script in its existing realm, applies any
  callback commands to its child-owned document, and transfers the resulting
  snapshot/effects through the existing protocol.
- Callback DOM mutations use the existing command validation and clone-before-
  commit path. Listener callbacks can therefore update form state or
  attributes without giving Rust a callback ABI or allowing the parent to
  evaluate a network document.
- A callback exception is returned as a typed script error. No callback source,
  form value, or sensitive event payload is logged or serialized into effects.
- The semantic action and callback mutation are separately revisioned: the
  action result reports the first committed action revision, and a callback
  mutation advances one additional revision when it exists.

## Deliberate boundary and tradeoffs

- This first bridge is post-action. It observes the committed action state;
  `preventDefault()` is reported to the realm but does not yet roll back or
  suppress the already-committed Rust default action.
- Callback failure happens after the action commit, so an exception cannot
  undo the action. The next preflight slice must define cancellation,
  default-action ordering, and one-revision transaction behavior.
- Scroll dispatch targets the document root; pointer/keyboard/IME event
  synthesis, trusted-event flags, composed paths, and browser task ordering
  remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- focused local semantic-action listener test
- focused child-process semantic-action listener test
- full native integration target — 289/289 passed.
- `git diff --check`

The next gate is action preflight: run cancelable click listeners before
activation, preserve default-action ordering, and fold callback mutations and
the accepted/rejected action into one atomic owner revision in local and child
documents.
