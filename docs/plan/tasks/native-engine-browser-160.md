# Native engine browser slice 160: same-origin frame DOM commands

Status: completed locally.

## Objective

Make same-origin embedded documents writable through the normal page-script
surface. A parent realm that obtains `iframe.contentDocument` must be able to
perform the ordinary bounded element operations already supported by a native
document, while cross-origin documents remain isolated.

## Contract

- A projected same-origin frame element can route focus, blur, click, value,
  selection, checked/selected state, custom validity, and attribute
  mutations to the child frame's owning native engine.
- The parent receives its normal JavaScript result immediately; the child
  mutation is committed through its own document/runtime owner and is visible
  on the next projection refresh.
- Child event handlers, default click behavior, navigation, and queued browser
  effects continue through the child engine's existing host path.
- The router validates both frame IDs, resolves the current source and target
  origins, rejects opaque or mismatched origins, and never transfers a native
  engine pointer or JavaScript object between realms.
- The content-worker wire carries frame commands as bounded, validated data;
  nested frame-command envelopes are rejected.

## Implementation

- Added a serializable `FrameScript` host command and bounded frame-command
  queue to the JavaScript runtime and `NativeEngine`.
- Added the content-worker response field and validation for routed frame
  commands.
- Added the child-side internal command entrypoint, which reuses the existing
  element methods so DOM effects and event behavior retain one implementation.
- Added backend routing for active and parked page/frame owners, including
  same-origin checks and bounded cascades for child-generated effects.
- Extended the HTTP same-origin frame coverage to verify parent-to-child and
  selected-child-to-parent writes persist in the real child document owner.

## Tradeoffs

This keeps the two-crate workspace and the existing process boundary. The
bridge is intentionally command-based rather than sharing live JavaScript
objects, so it avoids pointer lifetime and isolation hazards. It also means
only the already-supported bounded element operations are routed in this
slice; the next cross-realm work should extend event/listener identity and
structural DOM operations as complete behavioral units rather than creating a
second document implementation.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`

The focused HTTP frame test passed after the implementation and verifies
parent-to-child and selected-child-to-parent mutation persistence.

