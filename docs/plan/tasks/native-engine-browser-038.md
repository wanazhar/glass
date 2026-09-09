---
id: native-engine-browser-038
scope: glass-browser/native-engine/form-validation-submitter
status: done
depends-on: [native-engine-browser-037]
---

# BE-03o/BE-04t: bounded form validation and submitter metadata

## Objective

Close the next interactive-form boundary after urlencoded POST support:
required controls must block interactive submission, and submit callbacks must
identify the submitter that initiated them.

## Contract

- Local and child-owned form submissions initiated by a submit button or
  `requestSubmit()` validate the bounded required-control set before dispatching
  `submit`.
- Disabled and read-only controls do not block submission. Required text
  inputs, textareas, checkboxes, radio groups, and single-select controls are
  checked; unsupported constraint families remain explicit gaps.
- Each invalid control receives one non-bubbling, cancelable `invalid` event.
  Invalid events run before `submit`, and an invalid form does not navigate or
  dispatch `submit`.
- `requestSubmit(submitter)` validates that the argument is a submit control
  owned by the form. The resulting `submit` event exposes the bounded element
  snapshot as `event.submitter`; button activation supplies the clicked control.
- Direct `form.submit()` remains the event-free, validation-free submission
  path. Callback mutations remain inside the existing typed clone-and-commit
  owner boundary.

## Deliberate boundary and tradeoffs

- This is not full HTML constraint-validation parity. `ValidityState`,
  `checkValidity()`/`reportValidity()`, `setCustomValidity()`, pattern,
  minlength/maxlength, min/max/step, type-specific syntax, file controls,
  `formnovalidate`, external form association, and target contexts remain open.
- Submitter identity is delivered to the submit callback but is not yet added
  to successful-control serialization as a separate submitter name/value pair;
  image coordinates and multipart encoding remain open.
- Invalid callbacks may mutate the bounded document, but the current validation
  decision is not re-run within the same submission turn.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_form_ -- --nocapture` — 3/3 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_form_validation_blocks_submit -- --nocapture` — 1/1 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_form_submit -- --nocapture` — 2/2 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_semantic_submit_button_owns_get_navigation -- --nocapture` — 1/1 passed.
- `git diff --check`

The broader literal `form` filter also matched an unrelated preformatted-text
fixture whose existing inline JavaScript is syntactically invalid; that test
fails at initialization before form behavior runs and is not counted as this
slice's evidence. Remote CI, push, release, tag, registry publication,
browser-parity, and issue-closure claims remain pending the wider gates.
