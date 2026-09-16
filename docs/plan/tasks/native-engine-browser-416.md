# Native form POST target payloads (416)

status: complete
scope: native-engine/form-post-target-payloads
issue: 40
depends-on: [native-engine-browser-415, native-engine-browser-048]

## Objective

Carry the bounded form POST method, body, and content type through native
browsing-context navigation effects so `_parent`, `_top`, `_blank`, and named
targets submit the original request instead of rejecting it or silently
downgrading it to GET. Keep ordinary Window navigation GET-only and preserve
the existing form validation, `form-action`, opener, target reuse, history,
and content-process ownership contracts.

## Contract

- Form submission metadata includes the validated method, bounded body, and
  content type in local and content-process navigation results.
- Browser-owned frame and popup navigation effects preserve those fields;
  absent payload fields remain a backward-compatible GET with no body for
  non-form Window navigation.
- New popup targets initialize without issuing a duplicate GET, then perform
  the original POST exactly once. Existing named targets receive the same
  request payload.
- Parent/top frame navigation uses the same request method and body as the
  submitting form. The parent remains the navigation owner after the request
  crosses the content-process boundary.
- Payloads are bounded and validated before queue admission and again when a
  content-process response is decoded. Invalid method/body combinations fail
  closed; a GET body is never accepted as a POST substitute.
- No CDP path or silent fallback is introduced.

## Non-goals

This slice does not add multipart streaming, upload progress, arbitrary
non-form Window navigation methods, service-worker body replay, or full HTML
form submission conformance. It does not change form encoding or validation.

## Implementation path

- Add a serializable bounded navigation payload to popup/window effects and
  content-process form navigation records.
- Reconstruct and compare the transferred request against the parent document
  snapshot before dispatching it.
- Thread the payload through frame navigation, named-target reuse, and popup
  creation; initialize POST targets from `about:blank` to avoid a duplicate
  GET.
- Add local and HTTP witnesses for POST `_blank`, named reuse, and ancestor
  frame targets, including request body/method assertions and no-duplicate
  request counts.

## Tradeoffs

- A tagged body transfer is more explicit and safer than re-reading mutable
  form state after the child turn, but increases IPC/effect payload size by a
  bounded amount.
- Initializing POST popups from `about:blank` adds a local bootstrap turn but
  is required to avoid a visible duplicate GET and keeps request ownership
  deterministic.
- Reusing the existing navigation queues preserves ordering and target
  serialization, while body replay through service-worker/cache paths remains
  a separate conformance gate.

## Evidence

- `cargo check --quiet -p glass-browser --tests --locked` — passed.
- `cargo test --quiet -p glass-browser --test native_engine native_form_targets_keep_opener_and_reuse_named_targets -- --nocapture` — passed.
- `cargo test --quiet -p glass-browser --test native_engine native_nested_form_top_target_navigates_parent_context -- --nocapture` — passed with a POST ancestor navigation.
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_form_target_creates_popup_and_keeps_opener -- --nocapture` — passed.
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_form_post_target_preserves_payload -- --nocapture` — passed; the server observed exactly one POST with the expected content type and body.
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_named_form_post_reuses_target_without_get -- --nocapture` — passed; both submissions remained POSTs and the named target count stayed stable.
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_script_form_submit_sends_post_controls -- --nocapture` — passed.
- `cargo fmt --all` — passed.
- `git diff --check` — passed.

Documentation coverage, depth, and release-truth checks remain the final
checkpoint after this task is staged. Remote CI, push, release, tag, and
registry publication are outside this local checkpoint.

Remote CI, push, release, tag, and registry publication are outside this
local checkpoint.
