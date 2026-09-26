---
id: native-engine-browser-760
scope: glass-browser/element-internals-form-values
status: done
depends-on: [native-engine-browser-759]
---

# Glass native-engine browser slice 760: ElementInternals form values

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the
  source of truth for the native browser completion objective.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) requires
  HTML forms and custom elements. Slices [758](native-engine-browser-758.md)
  and [759](native-engine-browser-759.md) provide the autonomous registry and
  customized built-ins, but do not implement form-associated custom elements.
- The [HTML Standard custom-element section](https://html.spec.whatwg.org/multipage/custom-elements.html#form-associated-custom-elements)
  defines `formAssociated`, `attachInternals()`, and `setFormValue()`; the
  existing native FormData and multipart paths are reused here.

## Objective

Implement the bounded `ElementInternals` foundation that makes a declared
form-associated autonomous custom element contribute real values to
`new FormData(form)` and existing FormData request serialization. Invalid
attachment and unsupported input must fail explicitly. This is a useful
form-data capability, not a claim of complete form-associated custom-element
behavior.

## Contract

- `customElements.define()` reads and stores the constructor's
  `formAssociated` value atomically. A customized built-in may be defined, but
  it is not treated as a form-associated custom element; its
  `attachInternals()` call fails with `NotSupportedError`. Supported lifecycle
  callback properties are validated as
  callable. Until lifecycle dispatch is implemented, definitions that provide
  form-association, disabled, reset, or state-restore callbacks fail explicitly
  with `NotSupportedError` rather than silently accepting callbacks that will
  never run.
- `HTMLElement.attachInternals()` is available on upgraded/directly
  constructed autonomous custom elements, returns an `ElementInternals` object
  targeting that exact element, and can be called only once. Calls on ordinary
  elements, customized built-ins, elements without a definition, or a
  definition with `disabledFeatures: ["internals"]` fail with
  `NotSupportedError`. Calling it twice also fails with `NotSupportedError`.
  `ElementInternals` cannot be directly constructed by page code.
- `ElementInternals.form` exposes the current form owner for supported
  same-document custom elements: explicit `form` ID reference first, otherwise
  the nearest ancestor form. Missing or unresolved ownership returns null.
- `setFormValue(value[, state])` is available only for elements whose
  definition is form-associated. It accepts null, strings, native File values,
  and FormData. Null removes the control from successful entries. A string or
  File uses the element's `name`; a FormData value contributes its cloned,
  ordered entries and their own names. Omitted state snapshots the submission
  value; explicitly provided state is retained separately. Only genuine native
  File and FormData instances receive those union arms; other values use
  USVString conversion (Symbols and unconvertible values throw), and over-limit
  inputs fail explicitly.
- `new FormData(form)` emits form-associated custom-element entries in
  document order alongside existing successful built-in controls. It requires
  a matching form owner, excludes a custom element with its own `disabled`
  attribute and descendants disabled by a `fieldset` except descendants of its
  first `legend`, skips an absent/empty element name for scalar values, and
  preserves explicit FormData entry names. Existing multipart serialization
  and request bounds apply to these values.
- Internal state remains attached to the custom element across script
  evaluations in its current document/global owner; it is not inferred from or
  serialized into HTML attributes.

## Explicit boundaries

This slice does not claim complete ElementInternals Web IDL behavior. Form-owner
change callbacks, `formDisabledCallback`, `formResetCallback`/native reset,
`formStateRestoreCallback`, labels, custom
validity/constraint validation, accessibility semantics, custom states,
submission through native HTML form navigation, cross-document adoption of
form state, and Web Platform Tests remain open. A page that requests any
unsupported lifecycle hook receives an explicit definition error.

## Tradeoffs

The existing engine already owns ordered FormData entries, File values, and
bounded multipart request bodies, so this slice connects custom values to that
path without introducing a second form serializer or new crate. Form ownership
is computed from the current document tree and explicit `form` reference;
native submission/lifecycle ownership needs a later coordinated Rust/JS slice.
Failing definition when an unsupported form callback is present is stricter
than silently succeeding and omitting observable behavior, while the supported
`setFormValue` path remains useful for script-created FormData and fetch/XHR.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-760.md`

## Verification

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  passed; it reports the existing dead-code warnings in `native_engine/dom.rs`.
- `cargo test -p glass-browser --test native_engine collects_form_associated_custom_element_values --locked --quiet`
  passed: 2 tests, 0 failures, 0 ignored, 798 filtered (23.73 seconds).
- The in-process and content-process regressions cover `ElementInternals`
  branding, attach-once and disabled-feature failures, ordinary/customized-
  built-in rejection, form-owner lookup (ancestor and explicit `form`), scalar,
  File and cloned multi-entry FormData submission values, document-order
  composition with native controls, null/disabled/unnamed exclusions, disabled
  fieldset inheritance with the first-legend exception, explicit
  unsupported-callback errors, forged implementation markers treated as
  ordinary values instead of native File/FormData instances, and retained
  values after script refresh.
- `cargo test -p glass-browser --test native_engine native_custom_elements_upgrade_create_and_run_lifecycle_reactions --locked --quiet`
  passed: 1 test, 0 failures, 0 ignored, 799 filtered (19.39 seconds).
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Documentation gates passed: release truth (1,388 Markdown files, 83 current
  documents, 63 previous-version hits, 1,569 semantic-audit hits, zero stale
  current claims); depth (93 guides, 19 contracts); TUI shortcuts (15 keys,
  63 markers); and live coverage (1,388 Markdown files, 346 MCP tools,
  101 browser-only, 17 examples, 22 public modules).
- Native form-navigation submission, FACE lifecycle callbacks, custom
  validity, labels/accessibility, custom states, remote CI, cross-platform
  runtime certification, and issue #40 closure remain open.
