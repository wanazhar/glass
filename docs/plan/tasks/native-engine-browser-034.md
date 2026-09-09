---
id: native-engine-browser-034
scope: glass-browser/native-engine/form-submit-lifecycle
status: done
depends-on: [native-engine-browser-033]
---

# BE-03k/BE-04p: bounded GET form-submit lifecycle

## Objective

Complete the existing GET-form slice's default-action boundary: submit events
must be observable and cancelable, callback mutations must be included before
query serialization, and submit-button actions must hand navigation back to
the correct local or child-owned navigation owner.

## Contract

- `form.requestSubmit()` emits a bubbling, cancelable `submit` event before
  the bounded GET URL is serialized. `event.preventDefault()` suppresses the
  navigation while retaining callback mutations and the typed submit effect.
- Direct `form.submit()` retains its direct-submit behavior and bypasses the
  submit event. The host distinguishes it from `requestSubmit()` in the typed
  script command stream.
- Script-driven submit-button clicks and native semantic submit-button clicks
  use the same GET-only owner path. Local actions navigate through the local
  resource/history owner; child actions transfer a validated navigation record
  over IPC and the parent performs the next child-owned load.
- Submit-listener mutations land in the cloned document before named,
  enabled-control query serialization. Navigation is not emitted when the
  submit event is canceled, and the parent never falls back to CDP.

## Deliberate boundary and tradeoffs

- This remains a GET-only form contract. POST bodies, multipart encoding,
  constraint validation, submitter-specific name/value handling, target
  browsing contexts, unload/pagehide ordering, and form-associated custom
  elements remain open.
- Submit events are represented by bounded internal event metadata; raw form
  values and callback source do not enter effect records or IPC diagnostics.
- The content worker serializes its navigation handoff explicitly. A missing
  handoff is therefore a testable transport failure rather than a silent
  parent-side guess.

## Paths

- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_form_submit_event_can_cancel_request_submit -- --nocapture` — 1/1 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_semantic_submit_button_navigates_without_script_realm -- --nocapture` — 1/1 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_form_submit_navigates_with_get_controls -- --nocapture` — 1/1 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_semantic_submit_button_owns_get_navigation -- --nocapture` — 1/1 passed after fixing the child click navigation handoff serialization.
- `git diff --check`

The next form gates are POST/multipart bodies, validation and submitter
semantics, target contexts, and unload/navigation task ordering.
