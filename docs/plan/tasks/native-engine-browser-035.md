---
id: native-engine-browser-035
scope: glass-browser/native-engine/form-post-urlencoded
status: done
depends-on: [native-engine-browser-034]
---

# BE-03l/BE-04q: bounded urlencoded POST form navigation

## Objective

Extend the existing form default-action owner from GET-only navigation to the
bounded, interoperable `POST application/x-www-form-urlencoded` path without
creating a second network stack or allowing the parent to guess a child
navigation body.

## Contract

- A form with `method="post"` and the default or explicit
  `enctype="application/x-www-form-urlencoded"` serializes its enabled,
  named controls into one bounded request body.
- Direct `form.submit()`, `requestSubmit()`, and submit-button defaults retain
  the task-034 submit-event/cancellation contract; accepted callback
  mutations are serialized before the POST body is created.
- Local fixture navigation and sandboxed child navigation use the same typed
  `NativeNavigationRequest` contract. Child IPC transfers method, URL, and
  bounded body to the existing content-process loader; page values do not
  enter effects or diagnostics.
- The existing cookie, referrer, redirect, MIME, charset, CSP, mixed-content,
  response-size, and content-worker ownership paths remain authoritative.
  POST responses are not inserted into the GET document cache. A 301/302/303
  handoff converts POST to GET; 307/308 preserves the bounded POST request.
- Unsupported methods and `multipart/form-data` or `text/plain` encoding fail
  with typed unsupported errors rather than silently changing semantics.

## Deliberate boundary and tradeoffs

- This is urlencoded POST only. Constraint validation, submitter-specific
  name/value and image coordinates, multipart and text/plain bodies, target
  browsing contexts, popups, unload/pagehide ordering, and form-associated
  custom elements remain open browser-profile work.
- Fixture navigation cannot observe an HTTP body; the local gate verifies the
  typed form path and result navigation, while the child HTTP gate verifies
  the actual method, content type, and encoded request body.
- Request bodies are bounded to the existing Glass text budget and are never
  logged. Redirect policy deliberately follows the existing bounded chain and
  applies browser form POST-to-GET rules only for 301/302/303.

## Paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine post_controls -- --nocapture` — 2/2 passed.
- `git diff --check`

Remote CI, push, release, tag, registry publication, browser-parity, and
issue-closure claims remain pending explicit authorization and the wider
browser-complete gates.
