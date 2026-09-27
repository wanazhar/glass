---
id: native-engine-browser-763
scope: glass-browser/native-reset-button-activation
status: done
depends-on: [native-engine-browser-762]
---

# Glass native-engine browser slice 763: reset-button activation

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) is the source of
  truth for native browser completion.
- Slice [762](native-engine-browser-762.md) implements the shared form reset
  algorithm and its `reset` event / `formResetCallback()` lifecycle.
- The [HTML Standard button activation behavior](https://html.spec.whatwg.org/multipage/form-elements.html#the-button-element)
  resets the button's form owner for a reset button. The
  [input Reset Button state](https://html.spec.whatwg.org/multipage/input.html#reset-button-state)
  uses the same reset algorithm.

## Objective

Wire form-associated `button[type=reset]` and `input[type=reset]` activation
through the shared slice-762 reset path. Cover native BrowserSession clicks,
JavaScript `.click()`, and same-origin frame routing without submitting or
navigating.

## Contract

- Activation occurs only after an uncanceled bubbling `click` event. If a
  click listener changes the control's type or form owner, activation uses the
  post-listener state and current owner.
- Only a button in the Reset Button state or an input in the Reset Button state
  triggers this behavior. A button in Auto/Submit Button state, another input
  type, a disabled control, or a control without a form owner does not reset a
  form.
- The reset button invokes the existing form reset path exactly once. That path
  dispatches the bubbling, cancelable `reset` event; canceling that event
  preserves control state and suppresses `formResetCallback()` reactions.
- Native click and JavaScript `.click()` use the same behavior in top-level and
  same-origin frame Documents. Explicit `form` ownership follows the existing
  first-ID-in-tree-order rule.
- Reset activation does not dispatch `submit`, validate the form, or navigate.
  Activation observes type/form-owner changes made by click listeners. If the
  listener leaves the control without a current form owner, no form reset runs.

## Tradeoffs and boundaries

This slice reuses `nativeFormReset()` and the native `ResetForm` command so
event cancellation, default-state restoration, and custom-element reactions
cannot drift into a second implementation. General Enter/Space-generated button
clicks are part of the broader keyboard activation model and remain outside
this slice; tests must not imply keyboard activation support. Process-backed
Enter/Space activation is implemented separately in slice 764; inline/local
engine parity remains tracked in slice 765.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-763.md`

## Verification

Passed locally:

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine activates_reset_buttons --locked --quiet`
  (2 tests: top-level native semantic click and JavaScript `.click()`, plus
  same-origin-frame `.click()`)
- `cargo fmt --all -- --check`
- `git diff --check`
- Maintainer documentation gates: release-documentation audit, documentation
  depth, TUI shortcut inventory, and documentation coverage.

The regressions verify click cancellation, reset-event cancellation, current
type/form-owner changes made by click listeners, and no validation, submission,
or navigation. These slice-763 checks do not establish remote CI,
cross-platform certification, or issue #40 completion. Keyboard-generated
activation has separate process-backed evidence in slice 764; inline/local
engine parity remains open in slice 765.
