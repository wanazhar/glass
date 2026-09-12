# Native engine browser slice 241: focused nested-frame keyboard ownership

Status: completed locally.

## Objective

Preserve browser-like keyboard ownership after a pointer click enters an
embedded native browsing context. A point click can now reach a child frame,
but later key actions must follow the focused child without requiring an
immediate explicit frame-selection command.

## Contract

- Each active native page target records one bounded focused frame identity.
- A successful point click into a child frame records that child as the input
  owner; ordinary semantic actions in the selected frame update the owner to
  that frame as they do in a browser interaction session.
- `KeyDown`, `KeyUp`, `KeyPress`, and `Shortcut` actions route to a focused
  parked child frame while the public target context remains unchanged.
- Routed key actions use the child engine's existing content-process mutation,
  revision, event propagation, window-proxy, popup, navigation, and script
  effect paths.
- Frame navigation/rebuilds and detached focused frames reset ownership to the
  selected frame instead of retaining a stale child identity.
- Explicit frame selection establishes that frame as the current focus owner.

## Implementation

`NativeFrameState` now retains `focused_frame_id`. The backend performs a
bounded frame-tree reconciliation before focused-key routing, validates the
focused child is still attached, and applies the native action through the
route-aware frame engine pool. Child runtime effects are drained through the
same event and browser-effect coordinator used by direct frame actions.

Normal selected-frame actions update focus ownership, point-routed clicks set
the deepest child owner, and frame selection sets the explicitly selected
frame. Reconciliation clears ownership when a document generation rebuild
removes the focused child. The nested-frame action test now proves a click
into a two-level frame reaches a child text control and a subsequent key press
mutates that control while the parent remains the public context.

## Tradeoffs and follow-up

Focus ownership is target-local and bounded to the existing native frame tree;
it does not change the public selected-frame route used for DOM/script
inspection. Browser-level focus traversal, pointer capture, composition/IME,
selection ranges, and automatic locator resolution across frame boundaries
remain subsequent Issue #40 work. Child viewport sizing is now covered by the
following 242 slice.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_backend_routes_point_clicks_into_nested_frame_content -- --exact --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_backend_ -- --nocapture` (7 passed, 0 failed)
- `git diff --check`

Implementation checkpoint: local changes after `1a1b1005`; commit follows the
documentation gate.

Remote CI, push, release, tag, registry publication, browser parity, and
production-promotion claims are not made by this local checkpoint.
