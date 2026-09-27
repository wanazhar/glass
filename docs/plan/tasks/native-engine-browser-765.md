---
id: native-engine-browser-765
scope: glass-browser/inline-keyboard-button-activation
status: done
depends-on: [native-engine-browser-764]
---

# Glass native-engine browser slice 765: inline keyboard activation parity

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for browser completion.
- [Slice 764](native-engine-browser-764.md) implements Enter/Space button
  activation in process-backed network documents.
- The in-process/local `NativeEngine::action()` route bypasses the content
  worker bridge and currently emits key events without synthesizing button
  activation.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) requires
  one consistent native input contract across Glass entry paths.

## Objective

Implement keyboard button activation for local and in-process native Documents,
preserving the Enter/Space event phases and using the click/default-action
behavior for supported controls.

## Contract

- `NativeEngine::action()` and `action_async()` on local/non-process-backed
  Documents implement the slice-764 Enter and Space timing and cancellation
  rules for focused, enabled native buttons.
- Separate raw Space keydown/keyup actions retain at most one pending
  activation per Document; focus changes, detachment, disabled state, or
  document replacement suppress activation.
- Synthesized clicks preserve click listeners and their cancellation, dynamic
  type/form-owner changes, reset lifecycle, form validation, submit-event
  cancellation, submitter data, and allowed local form navigation.
- Non-navigating complete `Shortcut` actions retain one action revision; raw
  keydown and keyup remain separate actions. Successful form defaults retain
  the existing event-then-navigation commit behavior.
- The work does not broaden targets beyond the native controls listed in
  slice 764 or add anchors, arbitrary ARIA roles, IME/composition, key repeat,
  or implicit text-field submission.

## Tradeoffs and boundaries

This task closes the inline/local execution-path mismatch only. It does not
claim remote CI, cross-platform certification, complete keyboard conformance,
or issue #40 completion. Process-backed behavior remains governed by and
regressed against slice 764.

## Paths

- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-765.md`

## Verification

Passed locally:

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine native_local_keyboard_button_activation --locked --quiet`
  (5 tests covering synchronous/asynchronous actions, Enter/Space event order,
  keydown/click cancellation, raw Space focus/disablement/detachment/document
  replacement guards, live type/form-owner changes, reset cancellation/defaults,
  invalid/submit cancellation, submitter serialization, and fixture navigation)
- `cargo fmt --all -- --check`
- `git diff --check`
- Maintainer documentation gates: release-documentation audit,
  documentation-depth audit, TUI shortcut inventory, and documentation
  coverage.

The navigation regressions use only configured `fixture://` URLs. Form actions
still pass through the existing `NativeResourceLoader` form-action policy and
the local current-context navigation path; this slice does not add external
HTTP(S) transport, popup/context behavior, or broader URL-policy coverage.
Local checks do not establish remote CI, cross-platform certification, full
keyboard conformance, or issue #40 completion.
