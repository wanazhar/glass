# Native engine browser slice 240: nested native frame point routing

Status: completed locally.

## Objective

Make a normal native point click behave like a browser click when the point
falls inside an embedded browsing context. The preceding capture slice made
child content visible, but the backend still resolved the same point only in
the parent document.

## Contract

- Native point clicks first discover the selected frame tree and hit-test the
  parent frame-owner geometry.
- The deepest visible child frame containing the point receives the click,
  with coordinates translated through every frame boundary.
- Child actions retain their native engine ownership, revision, content-process
  mutation path, window-proxy synchronization, frame-event propagation, popup,
  navigation, and script-effect handling.
- Points outside embedded frames continue through the existing selected-frame
  action path.
- Parent overlays and fallback descendants are respected through the shared
  native hit-test/ancestor relationship instead of routing arbitrary covered
  points into a child.
- Partial frame visibility uses the same source offset for both capture and
  input, keeping scrolling and overflow clipping aligned.

## Implementation

`NativeEngineBackend` now resolves a recursive frame point route before the
ordinary action dispatcher. Candidate child frame owners are ordered by their
native stacking metadata; the deepest visible candidate is selected and its
local point is derived from the owner projection and source offset. The action
is applied through the existing route-aware native engine pool, and the
resulting events and queued browser effects are drained through the existing
frame propagation coordinator.

`NativeLayoutSnapshot` now exposes an internal projection pair containing the
visible rectangle and source-pixel offset. `NativeSurface` uses that offset
when copying a child surface, so a parent scroll or overflow clip cannot move
the child pixels relative to its owner. The document/engine projection also
provides the bounded ancestor check needed to distinguish an iframe owner from
an element layered above it.

## Tradeoffs and follow-up

Point routing remains integer-pixel and uses the existing bounded frame-tree
limit. It does not add a second event model or select a child frame as the
public active browsing context; explicit frame selection remains the route
control for DOM/script operations. Automatic focus-context routing for later
keyboard actions, frame-sized viewport negotiation, and complete compositor
transforms remain subsequent Issue #40 work.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_backend_routes_point_clicks_into_nested_frame_content -- --exact --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_backend_ -- --nocapture` (7 passed, 0 failed)
- `git diff --check`

Implementation checkpoint: local changes after `353af39a`; commit follows the
documentation gate.

Remote CI, push, release, tag, registry publication, browser parity, and
production-promotion claims are not made by this local checkpoint.
