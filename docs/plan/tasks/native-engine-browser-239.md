# Native engine browser slice 239: nested native frame surface composition

Status: completed locally.

## Objective

Make ordinary iframe and frame content visible in the parent native viewport.
Child browsing contexts already have independent native document owners and
script routes; this slice connects those owners to the parent screenshot path
without introducing a second renderer or a CDP capture dependency.

## Contract

- Async native capture discovers the selected frame's current child tree before
  raster capture, so callers do not need to call frame discovery first.
- Each live child frame is rasterized by its own native engine and composited
  into the visible layout box owned by its parent iframe/frame element.
- Composition recurses through the bounded frame tree, preserving nested frame
  content in the same parent viewport surface.
- The parent layout owns destination geometry and clipping; the child raster
  owns only its own bounded RGBA surface. No arena node, JavaScript object, or
  content-process pixel buffer crosses the frame boundary.
- Native runtime, CLI, MCP, and browser-backend capture routes use the async
  discovery/composition path. The existing synchronous low-level API remains
  available for callers that already manage frame discovery.
- Capability metadata no longer reports nested scrolling or IndexedDB as
  unavailable; its bounded current support is reflected in the public profile.

## Implementation

`NativeSurface` now has a bounded child-surface copy operation clipped to the
parent surface. `NativeEngineBackend` recursively rasterizes direct child
frames in layout order, finds each frame owner by its stable node index, and
copies the child viewport into the owner's visible parent rectangle. Async
capture reconciles the frame registry before this traversal. Runtime, CLI, and
MCP screenshot/observe paths call the async capture entry point.

The backend profile descriptions were synchronized with the completed nested
scroll/history and IndexedDB behavior. Existing capability levels and the
experimental certification level remain unchanged until the issue #40
promotion gates are complete.

## Tradeoffs and follow-up

Composition is integer-pixel, one-to-one, and clipped to the current frame
owner. The later 240 slice adds matching source-offset-aware point routing;
scaling, transforms, fractional geometry, compositor stacking integration with
parent overlays, and full browser frame-paint ordering remain later issue #40
work. The implementation keeps one surface owner per browsing context and
avoids duplicating layout or JavaScript state.

Frame discovery remains bounded by the existing topology limit. Missing,
detached, or not-currently-visible frame owners are skipped safely rather than
creating a stale pixel placement. Cross-origin script isolation and content
worker ownership are unchanged.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_backend_composes_nested_frame_surfaces_into_capture -- --exact --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_backend_ -- --nocapture` (6 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_runtime_session_owns_and_routes_child_frames -- --exact --nocapture` (1 passed, 0 failed)

Implementation checkpoint: `353af39a`.

Remote CI, push, release, tag, registry publication, browser-parity, and
production-promotion claims are not made by this local checkpoint.
