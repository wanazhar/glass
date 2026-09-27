---
id: native-engine-browser-764
scope: glass-browser/native-keyboard-button-activation
status: planned
depends-on: [native-engine-browser-763]
---

# Glass native-engine browser slice 764: keyboard button activation

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) is the source of
  truth for native browser completion.
- The [HTML Standard activation behavior](https://html.spec.whatwg.org/multipage/interaction.html#activation)
  requires keyboard activation to produce a click event.
- The [WAI-ARIA button pattern](https://www.w3.org/WAI/ARIA/apg/patterns/button/)
  identifies Enter and Space as button activation keys.
- Slice [763](native-engine-browser-763.md) routes pointer/semantic and
  JavaScript clicks through the native form reset and submit defaults.

## Objective

Implement Enter and Space activation for focused, enabled native button
controls. Reuse the click/default-action path in top-level and same-origin
frame Documents.

## Contract

- Enter activates on keydown. Space activates on keyup only when the matching
  keydown was not canceled and the same connected, enabled button still has
  focus. A canceled keydown does not activate the control.
- `Shortcut` dispatches one complete key sequence. Separate `KeyDown` and
  `KeyUp` actions retain only the bounded pending Space activation state needed
  to complete that sequence. A document replacement clears pending state.
- The activation target is an HTML `button` or an `input` whose current type is
  `button`, `reset`, `submit`, or `image`. Text controls, checkboxes, radios,
  ARIA-only buttons, and implicit Enter submission from a text field are outside
  this slice.
- Enter event order is `keydown`, synthesized `click`, then `keyup`. Space
  event order is `keydown`, `keyup`, then synthesized `click`. No pointer events
  are synthesized.
- The synthesized click uses the existing click/default-action pipeline. Its
  listeners can cancel activation or change the button type and form owner
  before defaults run. Reset buttons use the shared reset lifecycle. Submit
  buttons use current validation, submit-event cancellation, submitter data,
  and the normal form navigation path.
- The same contract applies in the top-level Document and selected
  same-origin-frame Document. A frame-local focus or pending key state must not
  activate a top-level control.
- Keyboard event cancellation and click cancellation are distinct: canceling
  keydown suppresses click dispatch; canceling the synthesized click suppresses
  its reset/submit default.

## Tradeoffs and boundaries

This slice models one outstanding Space press per content Document. It does not
add key repeat, IME/composition, keyboard activation for anchors or arbitrary
`role="button"` elements, or text-field implicit form submission. Those
behaviors remain separate profile work. The slice uses the existing Glass
keyboard action vocabulary and does not add a new public action type.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-764.md`

## Verification

Pending implementation. Required evidence includes focused process-backed
tests for Enter and Space phase order, canceled keydown/click, reset and submit
defaults, and top-level/same-origin-frame behavior; scoped package checking;
formatting; and the maintainer documentation gates. Local checks do not
establish remote CI, cross-platform certification, full keyboard conformance,
or issue #40 completion.
