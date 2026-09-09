---
id: native-engine-browser-048
scope: glass-browser/native-engine/form-submitter-overrides
status: done
depends-on: [native-engine-browser-047]
---

# BE-03y/BE-04ad: bounded submitter form overrides

## Objective

Make an activating submit control's `formaction`, `formmethod`, and
`formenctype` attributes participate in the same bounded form request that
already supports external ownership, validation bypass, and POST encodings.

## Contract

- A valid submitter overrides the owning form's action, method, and encoding
  independently; absent overrides retain the form attributes and defaults.
- Local and child-owned script submission preflight validates the effective
  submitter request rather than validating the form defaults first.
- Relative submitter actions resolve against the current document URL and the
  existing GET/POST, credential, size, redirect, and content-type policies
  remain in force.
- The existing `formnovalidate` behavior is unchanged. Target browsing
  contexts, dialog submission, file parts, and general form-control/Web IDL
  identity remain outside this bounded slice.

## Tradeoffs and boundary

This closes a common application form path without pretending that form
submission is complete. Overrides are applied only to the already-supported
GET and POST encodings; unsupported methods or encodings still fail closed.
The current engine remains one-context and does not implement `formtarget`.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `native_local_submitter_overrides_form_action_and_method`
- `native_content_process_submitter_overrides_form_action_method_and_encoding`
- existing `form_submission` tests: 2 passed
- existing `form_submit` tests: 5 passed
- existing `form_validation` tests: 2 passed
- `cargo fmt --all -- --check`
- `git diff --check`

Remote CI, source push, release, tag, registry publication, browser-parity,
and issue-closure claims remain pending the wider browser-complete gates.
