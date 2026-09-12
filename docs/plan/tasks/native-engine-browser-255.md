# Glass native engine browser slice 255: semantic action scroll-into-view

Status: completed locally.

## Objective

Align native semantic element actions with the existing Glass action contract:
when a resolved element is outside the active root viewport, bring it into
view before dispatching the action. This closes the child-frame regression
exposed when embedded frame viewports began using their real owner dimensions.

## Contract

- `Click`, `DoubleClick`, `Hover`, `Type`, `Clear`, `Check`, `Uncheck`, and
  `Select` resolve their semantic target and perform a bounded nearest root
  viewport scroll when the target has a layout box but no visible projection.
- Drag actions apply the same bounded root scroll step to both endpoints in
  source-then-destination order. The final destination remains visible for
  dispatch when the two targets share a document.
- Scroll coordinates supplied through `point=<x>,<y>` remain explicit. Point
  clicks never move the viewport or substitute a nearby target; an outside
  point returns the native typed action error.
- File upload and focused keyboard actions do not acquire a target viewport;
  upload retains its hidden-control behavior and keyboard actions retain their
  focused-frame behavior.
- The scroll is folded into the following accepted action state transition:
  it does not create a second document revision, and the current history entry
  records the resulting root offset. Read-only target preflight remains
  side-effect-free and continues to report `OutsideViewport` until an action
  is dispatched.
- Hidden, disabled, unsupported, stale, and non-layout targets still fail
  through their existing actionability checks after the scroll decision.

## Implementation

`NativeEngine::scroll_action_targets_into_view` resolves only semantic
targets, excludes point coordinates, and uses the current layout's document
rectangles plus bounded root maximum to compute the nearest visible offset.
The helper synchronizes local JavaScript/content-process scroll state before
the action owner performs its normal event, navigation, and mutation path.
`dispatch_locator_action` leaves point clicks to that native action path when
child-frame point routing does not claim them, preventing coordinate targets
from being misclassified as semantic locators.

The frame-runtime regression now verifies both successful child interaction
and a non-zero child viewport offset. The FormData constructor regression was
also synchronized with the newer file-control contract: an unselected file
control contributes the browser-compatible empty `File` value instead of the
obsolete fail-closed rejection assertion.

## Tradeoffs and follow-up

Root scrolling is intentionally bounded and nearest-edge based; it does not
yet emulate smooth scrolling, scroll anchoring, sticky/overlay hit-test
stability, or automatically scroll an inner CSS scroll container whose clip
still hides the target. Explicit point input remains coordinate-stable. The
scroll is state-synchronized without a separate revision so one semantic
action retains one revision boundary; detailed page `scroll` event ordering
and complete browser task-source parity remain part of the broader issue #40
promotion work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_form_data_constructor_collects_form_controls --locked -- --exact --test-threads=1 --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_point_click_uses_the_real_backend_dispatcher --locked -- --exact --test-threads=1 --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_runtime_session_owns_and_routes_child_frames --locked -- --exact --test-threads=1 --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine --locked -- --test-threads=1` (533 passed, 0 failed, 0 ignored)
- `git diff --check`

The evidence is local-only. Remote CI, push, release, registry publication,
and final native/CDP parity claims remain pending the wider issue #40 gates.
