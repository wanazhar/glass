---
id: native-engine-browser-872
scope: glass-browser/native-engine/bounded-capability-profile-truth
status: in_progress
depends-on: [native-engine-browser-871]
---

# Glass native-engine browser slice 872: capability-profile truth

## Objective

Correct stale user-facing native backend capability limitations so they
describe the bounded features that exist today without implying browser-wide
parity. Keep explicit gaps for standards-complete page loading, full Web IDL,
and scheduling behavior.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md` — native capability matrix
- `docs/plan/native-engine-browser-profile.md` — Glass Core Web Profile
- `crates/glass-browser/src/browser/native_backend.rs` —
  `NativeEngineBackend::profile_for`
- `crates/glass-browser/tests/native_engine.rs` — existing process-backed
  resource, worker, binary, and stream evidence
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Evidence crosswalk

| Capability claim | Current evidence | Slice 872 treatment |
|---|---|---|
| Navigation has no broad subresources | Page and worker script graphs plus parent-brokered stylesheet/CSS import/font, image, and media routes have process-backed coverage. | Name these as selected bounded resource paths; retain complete page-loading parity as open. |
| Script lacks binary and stream behavior | Page/Worker Fetch and XHR expose binary bodies; bounded Fetch request/response streams, clone/cancel, and buffered upload paths have targeted tests. | Describe byte and bounded stream support; retain full Fetch/Streams Web IDL parity as open. |
| Script lacks workers and subresources | DedicatedWorker, SharedWorker, ServiceWorker, WebSocket/EventSource, and selected resource owners have implemented/tested paths. | State bounded worker/resource slices rather than claiming broad absence or full parity. |
| Backend-profile metadata must stay testable | `NativeEngineBackend::profile_for` returns the public limitation text consumed by callers. | Add one unit test for current bounded claims and retained parity gaps. |

Relevant existing regressions include
`native_content_process_worker_fetch_preserves_binary_request_and_response_bodies`,
`native_content_process_fetches_stream_request_bodies_and_clones_them`,
`native_content_process_worker_drives_websocket_text_binary_and_close_events`,
`native_content_process_loads_static_http_media_subresource`, and
`native_content_process_mutation_stylesheets_use_parent_cookie_authority`.

## Contract

- Keep capability identifiers, support levels, portability, and runtime
  behavior unchanged; revise only inaccurate limitation descriptions.
- Navigation describes bounded HTTP(S) document/form flows and selected
  parent-brokered stylesheet/CSS import/font, image/media, and script/module
  resource paths. It must still say full browser-wide page-loading parity is
  open.
- Script describes bounded worker/API slices and parent-owned Fetch/XHR with
  binary and bounded stream behavior. It must still say full Web IDL identity,
  scheduling, and Fetch/Streams conformance are open.
- The architecture capability matrix uses matching scope and does not call
  implemented resource classes generally unsupported.
- A unit test checks the public profile's bounded support statements and
  retained parity boundaries so these specific stale claims cannot return.

## Tradeoff

This is a support-metadata and documentation correction, not a browser-feature
implementation or conformance claim. It makes current bounded support visible
to backend consumers while keeping the native engine's substantial parity gaps
explicit.

## Path

- `crates/glass-browser/src/browser/native_backend.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-872.md`
- `docs/plan/reviews/native-engine-browser-872-01.md`

## Verification

- Add a targeted unit test for `NativeEngineBackend::profile_for`.
- Run one scoped `cargo check` for `glass-browser --lib`, then only the exact
  unit test; reuse `/home/ubuntu/work/glass/target` and suppress successful
  build output.
- Run Rust formatting, `git diff --check`, and all four maintainer
  documentation gates after final doc edits. Use the existing shared-target
  CLI binaries explicitly for the coverage gate.
- Commit the design checkpoint first and the completed correction locally in a
  focused Conventional Commit. Do not push or claim remote CI.
- Update Issue #40 after the local checkpoint. Keep the epic open.

## Results

Implementation and verification are pending.
