---
id: native-engine-browser-028
scope: glass-browser/native-engine/form-get-submission
status: done
depends-on: [native-engine-browser-027]
---

# BE-03i/BE-04j: bounded GET form submission

## Objective

Connect the first ordinary form-submission path to the existing navigation
owner so script-driven forms can submit named controls in local and external
documents.

## Contract

- `HTMLFormElement.submit()` and `requestSubmit()` emit one typed owner command;
  clicking a submit button emits the existing click command and activates the
  same default GET submission path.
- The bounded encoder includes named, enabled text inputs, textareas, selected
  options, and checked checkbox/radio controls. Submit/reset/button/image
  controls are excluded unless the submit control itself activates the form.
  Query output is URL-encoded and capped by the existing locator budget.
- Only GET is accepted. The action resolves against the current document, strips
  credentials, and is handed to the local fixture/data loader or the parent
  network/content-process navigation owner. Child mutations transfer only a
  validated node index and final URL; the parent recomputes the form target
  after publishing the bounded mutation.
- A form command uses the existing clone-and-commit script transaction. No
  executable callback crosses IPC and no path silently falls back to CDP.

## Deliberate boundary and tradeoffs

- POST, multipart/text/plain encoding, image submit coordinates, constraint
  validation, submit/formdata events, `target` browsing contexts, unload/task
  ordering, history-state payloads, and upload controls remain open. This
  avoids pretending that a query-only handoff is full HTML form parity.
- Form controls are discovered only as descendants of the bounded form tree;
  external `form=` associations, shadow-tree controls, and custom elements are
  not represented yet.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine form_submit_navigates` — 2/2 passed.
- `git diff --check`

The next form/browser gates are POST and submission lifecycle/default-action
ordering, target browsing contexts, and constraint validation.
