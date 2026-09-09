id: native-engine-browser-059
scope: glass-browser/native-engine/form-validation-api
status: done
depends-on: [native-engine-browser-058]
---

# BE-03ad/BE-04ao: bounded form-validation API

## Objective

Expose the script-visible validation API over the same Rust validity owner used
by interactive submission, for both local and process-backed documents.

## Contract

- Form-associated controls expose bounded `validity` flags,
  `validationMessage`, and `willValidate`; form snapshots expose aggregate
  validity for `checkValidity()` and `reportValidity()`.
- `checkValidity()` and `reportValidity()` return the current bounded snapshot
  result and dispatch the existing ordered, non-bubbling `invalid` events for
  invalid controls without making `novalidate` suppress the API itself.
- `setCustomValidity()` stores a size-capped message through the existing
  typed local/child state wire and makes `customError`/`validationMessage`
  observable on the next refreshed host view.
- The API is available in both the local owner and sandboxed content-process
  owner; Rust remains the sole validity authority.

## Deliberate boundary and tradeoffs

This is a bounded snapshot/API surface, not full live Web IDL identity or
browser-localized validation UI. Pattern/file validation, picker state,
selection, and full `ValidityState` property/prototype/exception parity remain
outside the current profile. Submission `novalidate` and submitter
`formnovalidate` bypasses remain limited to submission, not explicit validity
API calls.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_form_validation_api -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_form_validation_api -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine form_validation -- --nocapture` — 7 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.
