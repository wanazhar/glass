# Native form target browsing contexts (415)

status: complete
scope: native-engine/form-target-contexts
issue: 40
depends-on: [native-engine-browser-414, native-engine-browser-048]

## Objective

Route bounded GET form submissions to the browsing context named by the form
or initiating submitter instead of always replacing the submitting document.
The native owner must preserve the existing submit, validation, form-action,
revision, and popup contracts while supporting `_self`, `_parent`, `_top`,
`_blank`, and named targets for local and HTTP(S) pages.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-048.md`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`

The native form owner already resolves external controls, submitter overrides,
validation, supported encodings, and `form-action` policy. It currently drops
the form target during navigation handoff, so a valid target silently becomes
the current browsing context. Links and `window.open()` already use the
browser-owned target/effect pipeline that this slice can reuse.

## Contract

- The form owner resolves `formtarget` on the submitter before the form's
  `target` attribute, with `_self` as the default. Reserved target keywords
  are case-insensitive; non-reserved names remain bounded named-context keys.
- A top-level `_self`, `_parent`, or `_top` submission navigates the current
  native target. A form inside a native child frame routes `_parent` and
  `_top` through the existing frame navigation effect queue.
- `_blank` creates one parked native target, and a named target reuses the
  existing named context through the backend's serialized target map. The
  opener stays selected and the returned target retains its normal opener
  identity and WindowProxy synchronization.
- The content process transfers the resolved target with its typed mutation;
  it does not expose raw DOM or script source to the parent. Enforced and
  report-only `form-action` checks happen before a new request/effect is
  emitted, and report records retain the 414 delivery ordering.
- This slice carries GET form navigations. POST target contexts remain an
  explicit typed follow-up until request bodies are added to the browser
  context navigation envelope; they must not silently downgrade to GET.
- No CDP path or silent fallback is introduced.

## Non-goals

This slice does not change form encoding, validation, submit-event ordering,
history policy, popup geometry, or WindowProxy identity. It does not claim
complete HTML target browsing-context semantics, POST-to-target body transfer,
download target behavior, or final Core Web Profile certification.

## Implementation path

- Preserve the resolved form target in the document-owned submission metadata
  and the content-process navigation envelope.
- Convert non-current GET form targets into the existing popup or frame-window
  navigation effects, with bounded target validation and opener ownership.
- Keep target reuse serialized by the backend's existing named-window map and
  effect scheduler.
- Add local and HTTP witnesses for `_blank`, named reuse, and nested
  `_parent`/`_top`; add a regression that a POST target does not issue a
  misleading GET.

## Tradeoffs

- Reusing the existing effect queues keeps target creation and selection
  atomic, but target forms incur one additional browser-owner turn.
- Target metadata adds a small bounded field to the content mutation wire and
  avoids copying the form element or private policy container to the parent.
- Keeping POST target contexts out of this envelope avoids data loss and a
  false compatibility claim, at the cost of one remaining form parity gap.

## Evidence

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine form_target --locked -- --nocapture` (2 passed)
- `cargo test --quiet -p glass-browser --test native_engine form_top_target --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine form_post_target --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine form_action --locked -- --nocapture` (6 passed)
- `cargo test --quiet -p glass-browser --test native_engine navigate_to --locked -- --nocapture` (3 passed)
- `cargo fmt --all`
- `git diff --check`

The witnesses cover local `_blank` and named reuse, HTTP(S) `_blank`, nested
`_top` ancestor routing with child promotion/cleanup, submitter `formtarget`
override, and typed rejection of non-current POST without a GET downgrade.
Remote CI, push, release, tag, and registry publication remain outside this
local checkpoint.
