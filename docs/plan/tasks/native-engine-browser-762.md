---
id: native-engine-browser-762
scope: glass-browser/native-form-reset-and-face-reset-reactions
status: in-progress
depends-on: [native-engine-browser-761]
---

# Glass native-engine browser slice 762: form reset

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) is the source of
  truth for the native browser completion objective.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) includes
  HTML forms and form-associated custom elements. Slice [760](native-engine-browser-760.md)
  implements `ElementInternals` form values; slice
  [761](native-engine-browser-761.md) implements form-owner and disabled-state
  lifecycle callbacks.
- The [HTML Standard reset algorithm](https://html.spec.whatwg.org/multipage/forms.html#resetting-a-form)
  dispatches a cancelable reset event before resetting owned controls. Reset
  button activation uses the same form-reset behavior. Form-associated custom
  elements receive `formResetCallback()` through custom-element reactions.

## Objective

Implement the native form-reset path as one ordered behavior: dispatch the
cancelable event, restore supported controls to their current defaults, and
run form-associated custom-element reset callbacks. Expose it through
`HTMLFormElement.reset()` and reset-button activation in top-level and
same-origin frame documents, in both in-process and content-process execution.

## Contract

- `form.reset()` is available on forms, returns `undefined`, and has no effect
  when the dispatched bubbling, cancelable `reset` event is canceled. It does
  not validate or navigate. Reentrant reset calls on a form already processing
  its reset are ignored.
- An uncanceled reset applies to the form's current resettable controls in
  document order, using their current form owner (including explicit `form`
  references). Controls whose owner changed earlier in the same script
  evaluation follow the ordered native command stream.
- Reset restores input values from the current `value` attribute, checkedness
  from the current `checked` attribute, textarea values from current child text,
  select option selectedness from current `selected` attributes with the
  supported single-select fallback, file inputs to an empty value and file
  list, and output values from their current default text. Existing native
  control-validity interaction state is reset without clearing author-provided
  custom validity.
- A `type=reset` button performs the same cancelable reset default action after
  its click event is allowed. Canceled click or reset events leave control
  values unchanged. Programmatic element `.click()` follows the same path.
- Callable `formResetCallback` is retained for form-associated autonomous
  custom elements. On an uncanceled reset, callbacks run through the existing
  bounded FIFO custom-element reaction queue after built-in controls are
  restored. Callback-issued control and `ElementInternals.setFormValue()`
  changes therefore take effect after the reset defaults. Invalid callback
  values still throw `TypeError`.
- The command path is shared by top-level and same-origin frame documents and
  preserves operations made by reset-event listeners before the reset default
  action and by reset callbacks afterward. Callback exceptions use existing
  reaction error reporting and do not abort later reset callbacks.
- `formStateRestoreCallback` remains explicitly unsupported with
  `NotSupportedError`; this slice does not claim state restoration.

## Explicit boundaries

This slice does not claim complete form-control Web IDL reflection, including
all `defaultValue`, `defaultChecked`, and `defaultSelected` property behavior;
complete per-input-type value sanitization; picker/UI behavior; reset behavior
for controls outside the native HTML control model; or full form-related Web
Platform Tests. These remain profile and issue #40 requirements. Remote CI,
cross-platform runtime certification, and browser-completion evidence remain
open.

## Tradeoffs

Reset intent is ordered as a native command after reset-event listener effects,
while JavaScript facades are updated immediately so custom callbacks observe
the reset values synchronously. Rust then applies the reset against the current
document tree and form-owner graph; callback commands follow it. This keeps
control state authoritative in the document owner while preserving browser
event order without a second independently maintained reset-state store.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-762.md`

## Verification

Pending implementation. Required evidence includes native reset cancellation
and ordering, supported input/textarea/select/file/output reset state, reset
button activation, form-associated callback ordering and errors, top-level and
same-origin-frame execution, focused cargo checks/tests, formatting, and local
documentation gates. Remote CI and cross-platform certification are not
established by local checks.
