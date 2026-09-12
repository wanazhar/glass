# Native engine browser slice 242: embedded-frame viewport negotiation

Status: completed locally.

## Objective

Give each embedded native browsing context the viewport it actually occupies in
its parent frame owner. Before this slice, child engines inherited the parent
configuration viewport and were only clipped during composition, which made
child layout and local hit-testing disagree with the visible iframe surface.

## Contract

- Child viewport width and height derive from the parent frame owner's content
  box, with the existing native viewport-dimension and device-scale limits.
- The negotiated viewport is used by the child layout, display list, raster,
  capture, hit-test, and action coordinate owners.
- Nested children negotiate independently from their immediate parent owner.
- Parent scrolling changes the projected destination and source offset, not the
  child viewport dimensions.
- Missing or temporarily non-layout frame owners retain the validated base
  viewport so frame discovery remains deterministic and bounded.

## Implementation

During native frame reconciliation, the backend derives a bounded `Viewport`
from each embedded owner content box and applies it to the child engine before
initialization. The inherited device-scale factor is preserved and dimensions
are clamped to `MAX_NATIVE_VIEWPORT_DIMENSION`. Since nested reconciliation
uses each newly created child as its parent, a multi-level frame tree gets
independent owner-sized viewports at every boundary.

## Tradeoffs and follow-up

The current negotiation is integer-pixel and uses the layout content box as
the frame viewport; fractional CSS geometry, transforms, browser chrome,
visual-viewport differences, and dynamic resize-observer delivery remain later
Issue #40 work. A frame without a usable layout box keeps the validated base
viewport and is omitted from visible composition/hit routing until geometry is
available.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_backend_composes_nested_frame_surfaces_into_capture -- --exact --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_backend_ -- --nocapture` (7 passed, 0 failed)
- `git diff --check`

Implementation checkpoint: local changes after `1440af9f`; commit follows the
documentation gate.

Remote CI, push, release, tag, registry publication, browser parity, and
production-promotion claims are not made by this local checkpoint.
