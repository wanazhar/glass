# Native CSP form-action enforcement (410)

status: done
scope: native-engine/csp-form-action
issue: 40

## Objective

Make the native browser enforce the CSP `form-action` navigation directive for
normal form submissions. A blocked action must stop before the native owner
issues the GET/POST navigation request, while valid submit handling and page
state remain intact.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-409.md`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- [W3C Content Security Policy Level 3](https://www.w3.org/TR/CSP3/)

The native runtime already parsed and enforced CSP for subresources, but form
submission navigation bypassed that policy owner. This made a page's explicit
`form-action` directive ineffective for local and HTTP content-process forms.

## Contract

- The shared CSP policy owner recognizes `form-action` and intersects all
  enforced response policies.
- `form-action` is an explicit navigation directive: when it is omitted, the
  policy does not fall back to `default-src`.
- Validated GET and POST submissions check the resolved form action after
  submit/default-action handling and before the native owner issues a request.
  This applies to local documents, process-backed click submission, and
  script-driven `form.submit()` submission.
- URL parsing, credentials, network-scheme, and existing CSP source matching
  remain owned by the shared resource loader. A blocked action leaves the
  current document URL and does not create a second network request.
- Report-only form-action decisions remain report-only and are recorded by the
  shared CSP queue; this slice does not claim complete event delivery for every
  form-event path.

## Non-goals

This slice does not implement the custom `navigate-to` policy, arbitrary
navigation-source coverage for links/history/popups, new form methods or
encodings, or complete report-only event delivery through every form path. It
does not change the existing form validation, submit-event, history, popup,
download, or target-context contracts.

## Implementation path

- Add a typed navigation-policy kind and `form-action` directive storage to
  the shared CSP owner.
- Resolve and check form actions through the loader in local script and
  non-script paths, and in the content-process click and script mutation
  paths.
- Preserve the existing submit event, validation, revision, and resource
  error behavior when enforcement rejects the navigation.
- Add a policy unit witness and an HTTP content-process witness covering both
  interactive click and script-driven submission with no follow-up request.
- Synchronize the architecture, active plan, analysis, and task evidence.

## Tradeoffs

- Keeping `form-action` as a separate typed policy kind prevents accidental
  reuse of subresource fallback semantics: `default-src` must not silently
  authorize a form destination.
- Checking after submit handling preserves page-observable submit behavior,
  but the blocked navigation has no separate page exception. This matches the
  existing native default-action model and leaves future violation-event
  delivery work isolated.
- The current form model supports GET/POST requests; this slice applies the
  policy to those existing requests without widening the form feature surface.

## Delivered

- Added shared enforced and report-only `form-action` policy decisions with
  multiple-policy intersection and explicit-directive semantics.
- Added local native checks for script/default form navigation and
  content-process checks for click and `form.submit()` navigation.
- Added regression coverage proving `default-src 'none'` alone does not block
  form submission and explicit `form-action 'none'` blocks both process paths
  before a second request.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib csp_form_action_is_explicit_and_intersected_across_headers --locked -- --nocapture` (1 passed)
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --test native_engine native_content_process_form_action_blocks_click_and_script_submit --locked -- --nocapture` (1 passed)
- Documentation validators for release metadata, documentation depth,
  shortcut coverage, and documentation coverage.

Remote CI, push, release, tag, and registry publication remain outside this
local checkpoint.
