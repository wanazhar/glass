# Native navigation report preflight across the content boundary (413)

status: complete
scope: native-engine/csp-navigation-report-preflight
issue: 40

## Objective

Carry report-only navigation observations from the HTTP(S) content owner to
the parent-owned top-level navigation boundary. A link, location assignment,
download, popup, history traversal, or direct navigation must expose the
child document's report-only `navigate-to` observation before the parent
allows, rejects, or commits the request.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-412.md`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- [W3C Content Security Policy Level 3](https://www.w3.org/TR/CSP3/)

Slices 400-403 established bounded report records, page dispatch, and
network delivery for resource owners inside the content process. Slice 411
and slice 412 made enforced `navigate-to` source groups visible to the parent,
but the parent still had no way to observe the child's report-only navigation
declarations before a parent-owned top-level decision. The `navigate-to`
vocabulary remains a Glass-owned policy extension in this repository; this
task does not present it as a normative CSP Level 3 directive.

## Contract

- The content worker exposes one versioned, typed navigation-policy preflight
  for the current document owner. It accepts only validated owner and target
  URLs and the Glass `navigate-to` policy kind.
- The child evaluates its own enforced and report-only policy container. The
  preflight returns the enforced result plus a bounded list of report-only
  `NativeCspViolation` records; report-only records never change the boolean
  authorization result.
- The parent dispatches returned records through the existing content page
  event channel before it completes the top-level policy decision. Listener
  effects use the normal mutation, storage, dialog, and navigation queues.
- Initial request preflight and final document ownership remain separate:
  the request is observed once before the outgoing lifecycle, the first load
  response does not duplicate that observation, and a later page-navigation
  handoff can be observed under its newly committed child policy.
- The existing parent source-group snapshot remains an independent enforced
  guard for target creation, downloads, popups, history, redirects, and final
  commits. No CDP or silent backend fallback is introduced.
- Missing policy state, missing report-only directives, and a child without a
  committed document are distinct from an explicit empty source list. No raw
  response headers cross the process boundary.

## Non-goals

This slice does not claim complete CSP grammar, a normative interpretation of
`navigate-to`, report-only meta-policy semantics beyond the existing live
container, new navigation APIs, redirect policy redesign, or full Core Web
Profile certification. It does not move navigation ownership into CDP or
make report-only policy enforceable.

## Implementation path

- Add a bounded `navigation_policy` content-worker command and decoder that
  returns the child decision and serialized violation records.
- Track whether the parent-side process has a committed child document so
  startup loads are not queried as though they had an outgoing page owner.
- Add an async parent policy helper that asks the child when it owns a
  network document and dispatches returned records through the existing page
  event path.
- Use the helper at async top-level preflight boundaries and keep the first
  content load's duplicate check silent; preserve synchronous/local behavior.
- Add an HTTP witness covering an enforced block plus report-only navigation
  event delivery, with no second document request.

## Tradeoffs

- A small IPC round trip is added before async top-level navigation from a
  process-backed document. This preserves the actual child policy owner and
  avoids copying private report declarations or endpoint metadata into the
  parent.
- Dispatching the record through a no-op page turn means a report listener
  can perform ordinary bounded page work before navigation, but it also makes
  navigation latency include that listener turn and its normal limits.
- The first load uses a silent parent-side snapshot check after the outgoing
  preflight, which keeps one report per attempted navigation while retaining
  the parent guard against a changed final URL. Redirect-report expansion is
  still a separate conformance decision.

## Verification

The local checkpoint passed:

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine navigate_to --locked -- --nocapture` (3 passed)
- `cargo test --quiet -p glass-browser --test native_engine form_action --locked -- --nocapture` (5 passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_reports_navigate_to_preflight_before_blocking_navigation --locked -- --nocapture` (1 passed)

Documentation verifiers pass after this status update. Remote CI, push,
release, tag, and registry publication remain outside this local checkpoint.
