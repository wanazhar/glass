# Native engine browser slice 171: propagated frame runtime effects

Status: completed locally.

## Objective

Carry child-frame runtime effects through navigation and `postMessage` into
same-origin parent and ancestor projections. The route must preserve the
browser event contract when a child changes state, including effects generated
by parent listeners while the projected event is being delivered.

## Contract

- The content worker exposes only bounded typed `NativeEffect` metadata,
  pending frame-script requests, browser-effect queues, and the current
  `window.name`; JavaScript objects and native document pointers do not cross
  the process boundary.
- Event targets use the child frame's committed node index, generation, and
  event kind. Window-level lifecycle mutations use the explicit window target
  sentinel rather than accidentally resolving the document root.
- Only same-origin frame ancestry can receive a projected event. The backend
  validates both origin and parent/child ancestry before dispatching to a
  parent or any higher same-origin ancestor.
- Original child effects are delivered once per eligible projected ancestor;
  parent-handler effects are captured as a new source and may continue up the
  same validated ancestor chain. Parent-issued focus/blur/click preflight is
  still not replayed.
- Frame scripts and browser-effect queues produced during propagation are
  drained by the existing bounded cascade after event dispatch, keeping
  navigation and `postMessage` side effects observable without unbounded
  recursive work.

## Implementation

- Added a bounded frame-runtime-effects bundle for navigation and
  `postMessage`, including effect deltas, pending scripts, browser queues, and
  synchronized `window.name`.
- Added ancestor-aware event propagation and effect capture for parent event
  handlers, with explicit origin and topology checks.
- Routed child navigation and `postMessage` through the propagated-effects
  path, preserving child-window lifecycle events and parent/ancestor
  projections.
- Corrected content-process lifecycle and hash-change event metadata to target
  the frame window, and allowed the engine validator to accept that explicit
  window target.
- Added HTTP witnesses for child-generated focus and lifecycle events,
  nested-grandchild ancestor projection, and the existing cancellation,
  replacement, and cross-origin security boundaries.

## Tradeoffs

The bridge remains typed and bounded, so it avoids sharing JavaScript objects
or native documents between processes at the cost of re-resolving projected
targets in the receiving realm. An effect's native node index is meaningful
only for its committed frame generation; full stale-generation rejection,
initial document load observer delivery, and complete Web IDL identity remain
subsequent issue #40 gates rather than being implied by this slice.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed, 440 filtered out)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_nested_frame_script_projection_preserves_window_chain -- --nocapture`
  (1 passed, 0 failed, 440 filtered out)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_orders_navigation_lifecycle_events -- --nocapture`
  (1 passed, 0 failed, 440 filtered out)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_beforeunload_can_cancel_replacement_navigation -- --nocapture`
  (1 passed, 0 failed, 440 filtered out)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_outgoing_lifecycle_can_replace_navigation -- --nocapture`
  (1 passed, 0 failed, 440 filtered out)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_cross_origin_frame_windows_enforce_security_boundary -- --nocapture`
  (1 passed, 0 failed, 440 filtered out)
- `git diff --check`
