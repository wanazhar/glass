---
id: native-engine-browser-761
scope: glass-browser/element-internals-owner-disabled-reactions
status: done
depends-on: [native-engine-browser-760]
---

# Glass native-engine browser slice 761: form-associated lifecycle reactions

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) is the source of
  truth for the native browser completion objective.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) includes
  HTML forms and custom elements. Slice [760](native-engine-browser-760.md)
  provides bounded `ElementInternals`, value submission, and form ownership.
- The [HTML Standard custom-element lifecycle](https://html.spec.whatwg.org/multipage/custom-elements.html#custom-element-reactions)
  defines form-owner and disabled-state callbacks. The existing native
  custom-element reaction queue is the dispatch path.

## Objective

Implement `formAssociatedCallback` and `formDisabledCallback` for autonomous
form-associated custom elements. Detect owner and disabled-state transitions
at upgrade and relevant DOM mutation boundaries, and dispatch callbacks using
the existing bounded custom-element reaction queue. Keep callbacks that lack
a native lifecycle path explicitly unsupported.

## Contract

- `customElements.define()` validates and retains callable
  `formAssociatedCallback` and `formDisabledCallback` properties for
  form-associated autonomous definitions. Invalid callback values fail with
  `TypeError`. `formResetCallback` and `formStateRestoreCallback` remain
  explicitly rejected with `NotSupportedError`.
- An upgraded form-associated custom element is associated using the same
  current-document resolver as `ElementInternals.form`. For an explicit
  `form` reference, the first element in tree order with the referenced ID
  owns the association only if it is a `form`; otherwise the owner is null.
  Without `form`, the nearest ancestor `form` is the owner.
- On upgrade, enqueue `formAssociatedCallback(form)` only if an owner exists.
  Enqueue `formDisabledCallback(true)` if the element is disabled. There is no
  initial callback for a null owner or an enabled element.
- After an owner has been established, each actual owner transition queues
  `formAssociatedCallback(newOwner)`, where `newOwner` is the form or null.
  Recomputing without a change must not duplicate callbacks. Transitions are
  detected for form-associated element insertion/removal/moves, its `form`
  attribute changes, relevant form ID changes, and tree insertions/removals
  that alter explicit ID resolution.
- `formDisabledCallback(disabled)` runs only when the computed disabled state
  changes. The state includes the custom element's own `disabled` attribute
  and disabled fieldset ancestors, except descendants of each fieldset's
  first direct `legend` child. Relevant attribute and tree mutations are
  reconciled in document order.
- Callback reactions are queued as one bounded batch after the triggering
  mutation is internally consistent. Existing FIFO reaction behavior,
  exception reporting, reentrancy handling, and queue/work limits remain in
  force. Callback exceptions do not corrupt the retained owner/disabled
  baseline or prevent later reactions.
- Owner and disabled baselines survive script evaluation refreshes in their
  current document/global owner. A refresh without a DOM transition does not
  re-fire callbacks.
- Customized built-ins, scoped registries, cross-global adoption, form reset,
  state restoration, custom validity, labels/accessibility, and custom states
  are not added by this slice.

## Explicit boundaries

This slice does not implement `formResetCallback` or the
`HTMLFormElement.reset()` algorithm, nor does it claim complete form-associated
custom-element behavior. Definitions that provide the still-unsupported reset
or state-restore callback continue to fail explicitly. Form navigation,
submission encoding, and the existing request/event path are unchanged.

## Tradeoffs

A bounded document-tree reconciliation after relevant mutations is simpler and
less error-prone than maintaining a second incremental form-owner graph in
Rust and JavaScript. Its work is capped by the existing DOM-node and reaction
limits. It does extra work for rare form/ID/disabled mutations, while avoiding
stale ownership after ancestor moves or duplicate-ID changes. The existing
reaction queue preserves the engine's established callback error and
reentrancy behavior.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-761.md`

## Verification

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  passed; the existing dead-code warnings in `native_engine/dom.rs` remain.
- `cargo test -p glass-browser --test native_engine collects_form_associated_custom_element_values --locked --quiet`
  passed: 2 tests, 0 failures, 0 ignored, 798 filtered (26.27 seconds).
- `cargo test -p glass-browser --test native_engine native_custom_elements_upgrade_create_and_run_lifecycle_reactions --locked --quiet`
  passed: 1 test, 0 failures, 0 ignored, 799 filtered (20.01 seconds).
- The in-process and content-process tests cover initial owner/disabled
  callbacks, explicit and ancestor ownership, owner transitions from form and
  ID changes (including an earlier duplicate non-form ID), own and inherited
  disabled transitions, the first-legend exception, removal/reinsertion,
  invalid/unsupported callback definitions, and no duplicate callbacks after
  script refresh.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Documentation release-truth, depth, TUI-shortcut, and live-coverage gates
  passed locally. Remote CI and cross-platform runtime certification remain
  open; these local checks do not establish issue #40 completion.
