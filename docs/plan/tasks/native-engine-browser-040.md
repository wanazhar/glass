---
id: native-engine-browser-040
scope: glass-browser/native-engine/form-submitter-serialization
status: done
depends-on: [native-engine-browser-039]
---

# BE-03q/BE-04v: bounded submitter serialization and validation bypass

## Objective

Complete the next successful-control form boundary by carrying the initiating
submit control into query/body serialization and honoring the two bounded
validation bypass attributes.

## Contract

- A successful submit button contributes its non-empty `name` and bounded
  `value` to GET query serialization and
  `application/x-www-form-urlencoded` POST bodies in DOM order.
- Local and child owners carry the submitter node through the typed navigation
  record. The parent recomputes and validates the final request from its own
  committed document before navigation, so the child cannot smuggle an
  arbitrary URL or submitter identity.
- A form with `novalidate` or an initiating submit control with
  `formnovalidate` bypasses the bounded required-control validation pass while
  retaining normal submit-event and serialization behavior.
- Submitter arguments still must be submit controls owned by the target form;
  malformed or detached submitters fail closed.

## Deliberate boundary and tradeoffs

- This does not claim image-submit coordinate pairs, multiple submitter
  controls, external `form=` association, target contexts, multipart or
  `text/plain` encoding, full constraint-validation APIs, or complete Web IDL
  `FormData` behavior.
- Button submitter values are bounded strings and the native engine retains its
  existing control-count, locator, URL, and request-body quotas.
- Direct `form.submit()` remains validation-free and contributes no submitter,
  matching its event-free direct-call boundary.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_form_ -- --nocapture` — 3/3 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_form_submit_navigates_with_get_controls -- --nocapture` — 1/1 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_form_submit_sends_post_controls -- --nocapture` — 1/1 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_semantic_submit_button_owns_get_navigation -- --nocapture` — 1/1 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_form_validation_blocks_submit -- --nocapture` — 1/1 passed.
- `git diff --check`

Remote CI, push, release, tag, registry publication, browser-parity, and
issue-closure claims remain pending the wider browser-complete gates.
