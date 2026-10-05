---
id: native-engine-browser-872
scope: glass-browser/native-engine/bounded-capability-profile-truth
status: done
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

`NativeEngineBackend::profile_for` previously claimed binary/stream behavior,
workers, and subresources were open, and the architecture matrix said general
subresources remained open. The current implementation has bounded support in
each area. The profile now names bounded Fetch/CORS and XHR text/binary bodies,
FormData/File/Blob/URLSearchParams, bounded streams, Worker API slices,
WebSocket/EventSource transports, and selected script/style/image/media
resources. Navigation metadata names selected parent-brokered stylesheets,
CSS imports/fonts, images/media, and classic/module resources. Both descriptors
retain complete page-loading, parser timing, Web IDL, scheduler, and
browser-wide conformance gaps. Capability IDs, support levels, and runtime
behavior are unchanged.

The first unit execution exposed the backend-profile validator's 256-byte
limit on each limitation entry. The bounded-support and remaining-gap claims
are now separate concise entries. The final exact unit test passes.

- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-browser --features native-engine --lib --locked --quiet` passed with successful output suppressed.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-browser --features native-engine --lib --locked --quiet browser::native_backend::tests::native_backend_profile_describes_bounded_navigation_and_script_support -- --exact` passed (1 passed; 1,695 filtered).
- `rustfmt --edition 2024 --check crates/glass-browser/src/browser/native_backend.rs` and `git diff --check` passed.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-872.json` passed (1,527 Markdown documents; 83 current; zero current-claim failures).
- `python3 scripts/check-documentation-depth.py` passed (93 guides and 19 contracts).
- `python3 scripts/check-tui-shortcuts.py` passed (15 keys and 63 documentation markers).
- `python3 scripts/check-documentation-coverage.py --glass /home/ubuntu/work/glass/target/debug/glass --glass-browser /home/ubuntu/work/glass/target/debug/glass-browser` passed (1,527 Markdown files; 346 full-product MCP tools, 101 browser-only; 17 examples; 22 public modules). It used the existing shared-target binaries via explicit paths.

This correction improves capability-reporting truth only; it is not new
browser conformance. Issue #40 remains open.
