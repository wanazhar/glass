# Native engine browser slice 170: cross-process frame event observation

Status: completed locally.

## Objective

Carry child-owned native event effects across the content-worker/backend
boundary into the same-origin parent projection, so event listeners installed
on a projected frame document or node observe activity originating in the
child realm.

## Contract

- Child frame commands and selected-frame navigation, actions, and scripts
  expose only bounded `NativeEffect` metadata to the backend; no JavaScript
  object or native document pointer crosses the process boundary.
- The parent refreshes the target frame binding before dispatch, resolves the
  child node index against that binding, and dispatches through the existing
  frame-local capture/target/bubble path to the projected document and window.
- Event delivery is limited to a direct same-origin child/parent relationship;
  cross-origin frames and commands targeting an ancestor do not gain an event
  observation channel.
- Projected focus, blur, and click preflight remains authoritative for a
  parent-issued frame command. Its already-delivered parent-side event is not
  replayed when the child command returns the corresponding effect.
- Parent listeners may continue to produce ordinary typed DOM/frame/browser
  effects; those queues are drained through the existing bounded cascade.

## Implementation

- Added bounded frame-event source generation with typed event metadata and
  context validation.
- Added a private frame-document node resolver for element, text, document,
  and window event targets, then installed `__glassDispatchFrameEvents` in the
  parent JavaScript realm.
- Captured child effect deltas around frame commands and routed them to the
  actual parent engine across active and parked target/frame arrangements.
- Captured effects from selected-frame navigation, semantic actions, and
  scripts, allowing content-worker events to reach the parent projection.
- Refreshed parent frame bindings before dispatch and preserved bounded nested
  frame-script and browser-effect processing.
- Added an HTTP content-worker witness that selects a child realm, triggers a
  child click, and verifies parent capture/bubble/window observation without
  top-level leakage.

## Tradeoffs

The bridge transfers event kind and native node identity rather than sharing
objects, which keeps the Rust ownership boundary and process isolation intact.
The parent receives a fresh projection when a child revision changes, so
listener ownership is stable but JavaScript object identity across a document
refresh is not yet a complete Web IDL guarantee. Event ordering for every
navigation/lifecycle and input path, observer APIs, and full browser parity
remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed, 440 filtered out)
- `git diff --check`
